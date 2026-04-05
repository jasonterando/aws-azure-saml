pub mod saml;
pub mod sts;

pub use saml::{create_saml_request, parse_saml_response, AwsRole};
pub use sts::assume_role_with_saml;
