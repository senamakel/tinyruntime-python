//! Unit tests for the crate-wide error type.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::Error;

#[test]
fn messages_are_lowercase_and_unpunctuated() {
    let errors = [
        Error::UnsupportedHost {
            os: "plan9".to_string(),
            arch: "x86_64".to_string(),
        },
        Error::InvalidVersion {
            value: "latest".to_string(),
            bound: "minimum",
        },
        Error::IndexUnavailable("the request timed out".to_string()),
        Error::NoMatchingBuild {
            release: "20240909".to_string(),
            bounds: ">= 3.12".to_string(),
        },
    ];
    for error in errors {
        let rendered = error.to_string();
        assert!(
            !rendered.ends_with('.'),
            "`{rendered}` ends with punctuation"
        );
        let first = rendered.chars().next().expect("a non-empty message");
        assert!(!first.is_uppercase(), "`{rendered}` starts with a capital");
    }
}

#[test]
fn an_unreadable_index_and_an_empty_one_are_different_errors() {
    // They call for different responses: one is worth retrying, the other means
    // the version bounds excluded everything the channel actually publishes.
    let unreadable = Error::IndexUnavailable("the connection failed".to_string()).to_string();
    let empty = Error::NoMatchingBuild {
        release: "20240909".to_string(),
        bounds: ">= 3.99".to_string(),
    }
    .to_string();
    assert!(
        unreadable.contains("could not be read"),
        "got `{unreadable}`"
    );
    assert!(empty.contains("no build matching"), "got `{empty}`");
    assert!(
        empty.contains(">= 3.99"),
        "the bounds that excluded everything are named"
    );
}

#[test]
fn an_invalid_bound_says_which_bound_it_was() {
    let rendered = Error::InvalidVersion {
        value: "nonsense".to_string(),
        bound: "maximum",
    }
    .to_string();
    assert!(rendered.contains("maximum"), "got `{rendered}`");
    assert!(rendered.contains("`nonsense`"), "got `{rendered}`");
}
