use super::*;

#[test]
fn initials_use_first_and_last_words() {
    assert_eq!(initials("Ada Lovelace", "a@example.com"), "AL");
    assert_eq!(initials("Jean Paul Sartre", "j@example.com"), "JS");
    assert_eq!(initials("cher", "c@example.com"), "C");
}

#[test]
fn initials_fall_back_to_email() {
    assert_eq!(initials("", "zed@example.com"), "Z");
    assert_eq!(initials("   ", ""), "");
}

#[test]
fn author_color_is_stable_and_case_insensitive() {
    assert_eq!(
        author_color("Ada@Example.com"),
        author_color("ada@example.com ")
    );
    assert_eq!(author_color("x@example.com"), author_color("x@example.com"));
}

#[test]
fn gravatar_url_hashes_the_normalized_email() {
    // SHA-256 of "myemailaddress@example.com", the example from Gravatar's documentation.
    assert_eq!(
        gravatar_url(" MyEmailAddress@example.com ", 40),
        "https://gravatar.com/avatar/84059b07d4be67b806386c0aad8070a23f18836bbaae342275dc0a83414c32ee?s=40&d=404"
    );
}
