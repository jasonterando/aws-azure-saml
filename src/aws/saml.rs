use crate::error::{AzureLoginError, Result};
use base64::{engine::general_purpose, Engine as _};
use chrono::Utc;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::Write;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AwsRole {
    pub role_arn: String,
    pub principal_arn: String,
}

/// Create a SAML AuthnRequest
pub fn create_saml_request(
    app_id_uri: &str,
    _tenant_id: &str,
    assertion_consumer_url: &str,
) -> Result<String> {
    let request_id = Uuid::new_v4();
    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S.%3fZ").to_string();

    let saml_xml = format!(
        r#"<samlp:AuthnRequest xmlns="urn:oasis:names:tc:SAML:2.0:metadata" ID="id{}" Version="2.0" IssueInstant="{}" IsPassive="false" AssertionConsumerServiceURL="{}" xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol">
    <Issuer xmlns="urn:oasis:names:tc:SAML:2.0:assertion">{}</Issuer>
    <samlp:NameIDPolicy Format="urn:oasis:names:tc:SAML:1.1:nameid-format:emailAddress"></samlp:NameIDPolicy>
</samlp:AuthnRequest>"#,
        request_id, timestamp, assertion_consumer_url, app_id_uri
    );

    // Deflate compress
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(saml_xml.as_bytes())
        .map_err(|e| AzureLoginError::ConfigError(e.to_string()))?;
    let compressed = encoder
        .finish()
        .map_err(|e| AzureLoginError::ConfigError(e.to_string()))?;

    // Base64 encode
    let encoded = general_purpose::STANDARD.encode(&compressed);

    Ok(encoded)
}

/// Parse roles from SAML response
pub fn parse_saml_response(saml_response: &str) -> Result<Vec<AwsRole>> {
    // Base64 decode
    let decoded = general_purpose::STANDARD
        .decode(saml_response)
        .map_err(|e| AzureLoginError::SamlParsingError(format!("Base64 decode error: {}", e)))?;

    let xml = String::from_utf8(decoded)
        .map_err(|e| AzureLoginError::SamlParsingError(format!("UTF-8 decode error: {}", e)))?;

    // Parse XML to extract roles
    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);

    let mut roles = Vec::new();
    let mut in_role_attribute = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if e.name().as_ref() == b"Attribute" {
                    // Check if this is the Role attribute
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"Name"
                            && attr.value.as_ref() == b"https://aws.amazon.com/SAML/Attributes/Role"
                        {
                            in_role_attribute = true;
                        }
                    }
                }
            }
            Ok(Event::Text(e)) if in_role_attribute => {
                let text = e.unescape().map_err(|e| {
                    AzureLoginError::SamlParsingError(format!("Text unescape error: {}", e))
                })?;

                // Parse role ARN and principal ARN
                // Format: "arn:aws:iam::123456789012:role/RoleName,arn:aws:iam::123456789012:saml-provider/ProviderName"
                // or reversed
                let parts: Vec<&str> = text.split(',').collect();
                if parts.len() == 2 {
                    let (role_arn, principal_arn) = if parts[0].contains(":role/") {
                        (parts[0].trim(), parts[1].trim())
                    } else {
                        (parts[1].trim(), parts[0].trim())
                    };

                    roles.push(AwsRole {
                        role_arn: role_arn.to_string(),
                        principal_arn: principal_arn.to_string(),
                    });
                }
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"Attribute" => {
                in_role_attribute = false;
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(AzureLoginError::SamlParsingError(format!(
                    "XML parsing error: {}",
                    e
                )))
            }
            _ => {}
        }
        buf.clear();
    }

    if roles.is_empty() {
        return Err(AzureLoginError::NoRolesFound);
    }

    Ok(roles)
}
