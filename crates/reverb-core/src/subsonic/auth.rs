use std::fmt::Write as _;

use md5::{Digest, Md5};
use rand::distr::{Alphanumeric, SampleString};

pub const API_VERSION: &str = "1.16.1";
pub const CLIENT_NAME: &str = "Reverb";

/// Per-request Subsonic auth parameters: a fresh salt and the salted
/// password token, plus the protocol boilerplate (v, c, f).
pub fn auth_params(username: &str, password: &str) -> Vec<(&'static str, String)> {
    let salt = Alphanumeric.sample_string(&mut rand::rng(), 12);
    let token = md5_hex(&format!("{password}{salt}"));
    vec![
        ("u", username.to_owned()),
        ("t", token),
        ("s", salt),
        ("v", API_VERSION.to_owned()),
        ("c", CLIENT_NAME.to_owned()),
        ("f", "json".to_owned()),
    ]
}

fn md5_hex(input: &str) -> String {
    let digest = Md5::digest(input.as_bytes());
    let mut out = String::with_capacity(32);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_matches_subsonic_docs_example() {
        // Example from the Subsonic API docs: md5("sesame" + "c19b2d")
        assert_eq!(md5_hex("sesamec19b2d"), "26719a1196d2a940705a59634eb18eab");
    }

    #[test]
    fn params_contain_all_required_keys() {
        let params = auth_params("admin", "secret");
        for key in ["u", "t", "s", "v", "c", "f"] {
            assert!(params.iter().any(|(k, _)| *k == key), "missing {key}");
        }
        let salt = &params.iter().find(|(k, _)| *k == "s").unwrap().1;
        assert_eq!(salt.len(), 12);
    }
}
