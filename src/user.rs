use std::ffi::{OsStr, OsString};
use uzers::os::unix::UserExt;

/// Returns the real UID of the calling process.
#[must_use]
pub fn current_uid() -> u32 {
    uzers::get_current_uid()
}

/// Parses a pkexec-style user specifier into a numeric UID.
///
/// Accepts plain decimal integers (`"1000"`) and the hash-prefixed form
/// used by some polkit APIs (`"#1000"`).  Returns `None` for bare names,
/// non-numeric strings, out-of-range values, or non-UTF-8 input.
#[must_use]
pub fn parse_uid_spec(user: &OsString) -> Option<u32> {
    let s = user.to_str()?;
    let s = match s.strip_prefix('#') {
        Some("") => return None,
        Some(rest) => rest,
        None => s,
    };
    s.parse().ok()
}

/// Looks up the home directory for a user by their login name.
///
/// Returns `None` if no such user exists or if the home path is empty.
#[must_use]
pub fn lookup_home_by_name(name: &OsStr) -> Option<OsString> {
    let home = uzers::get_user_by_name(name.to_str()?)?
        .home_dir()
        .as_os_str()
        .to_os_string();
    if home.is_empty() { None } else { Some(home) }
}

/// Looks up the home directory for a user by their numeric UID.
///
/// Returns `None` if no such user exists or if the home path is empty.
#[must_use]
pub fn lookup_home_by_uid(uid: u32) -> Option<OsString> {
    let home = uzers::get_user_by_uid(uid)?
        .home_dir()
        .as_os_str()
        .to_os_string();
    if home.is_empty() { None } else { Some(home) }
}

/// Resolves the home directory for a pkexec-style user specifier.
///
/// If the specifier is a numeric UID (`"0"`, `"#1000"`, …) it dispatches to
/// [`lookup_home_by_uid`]; otherwise it falls through to [`lookup_home_by_name`].
#[must_use]
pub fn lookup_target_home(user: &OsString) -> Option<OsString> {
    if let Some(uid) = parse_uid_spec(user) {
        return lookup_home_by_uid(uid);
    }
    lookup_home_by_name(user.as_os_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[must_use]
    fn user_is_root(user: &OsString) -> bool {
        match parse_uid_spec(user) {
            Some(uid) => uid == 0,
            None => user.as_os_str() == OsStr::new("root"),
        }
    }

    #[test]
    fn uid_spec_plain_zero() {
        assert_eq!(parse_uid_spec(&OsString::from("0")), Some(0));
    }
    #[test]
    fn uid_spec_plain_number() {
        assert_eq!(parse_uid_spec(&OsString::from("1000")), Some(1000));
    }
    #[test]
    fn uid_spec_hash_prefix() {
        assert_eq!(parse_uid_spec(&OsString::from("#1000")), Some(1000));
    }
    #[test]
    fn uid_spec_hash_zero() {
        assert_eq!(parse_uid_spec(&OsString::from("#0")), Some(0));
    }
    #[test]
    fn uid_spec_name_returns_none() {
        assert_eq!(parse_uid_spec(&OsString::from("root")), None);
        assert_eq!(parse_uid_spec(&OsString::from("alice")), None);
    }
    #[test]
    fn uid_spec_negative_returns_none() {
        assert_eq!(parse_uid_spec(&OsString::from("-1")), None);
    }
    #[test]
    fn uid_spec_u32_overflow_returns_none() {
        assert_eq!(parse_uid_spec(&OsString::from("4294967296")), None);
    }
    #[test]
    fn uid_spec_non_utf8_returns_none() {
        use std::os::unix::ffi::OsStringExt;
        let bad = OsString::from_vec(vec![0xFF, 0xFE]);
        assert_eq!(parse_uid_spec(&bad), None);
    }
    #[test]
    fn uid_spec_hash_only_returns_none() {
        assert_eq!(parse_uid_spec(&OsString::from("#")), None);
    }

    #[test]
    fn root_by_name() {
        assert!(user_is_root(&OsString::from("root")));
    }
    #[test]
    fn root_by_uid_zero() {
        assert!(user_is_root(&OsString::from("0")));
    }
    #[test]
    fn root_by_hash_uid_zero() {
        assert!(user_is_root(&OsString::from("#0")));
    }
    #[test]
    fn non_root_name() {
        assert!(!user_is_root(&OsString::from("alice")));
    }
    #[test]
    fn non_root_uid() {
        assert!(!user_is_root(&OsString::from("1000")));
    }

    #[test]
    fn lookup_target_home_by_name_root() {
        assert!(lookup_target_home(&OsString::from("root")).is_some());
    }
    #[test]
    fn lookup_target_home_by_numeric_uid_zero() {
        assert!(lookup_target_home(&OsString::from("0")).is_some());
    }
    #[test]
    fn lookup_target_home_by_hash_uid_zero() {
        assert!(lookup_target_home(&OsString::from("#0")).is_some());
    }
    #[test]
    fn lookup_target_home_all_root_spellings_agree() {
        assert_eq!(
            lookup_target_home(&OsString::from("root")),
            lookup_target_home(&OsString::from("0"))
        );
        assert_eq!(
            lookup_target_home(&OsString::from("0")),
            lookup_target_home(&OsString::from("#0"))
        );
    }
    #[test]
    fn lookup_target_home_nonexistent_returns_none() {
        assert!(
            lookup_target_home(&OsString::from("thisuserdoesnotexist_run0shimtest_xyzzy"))
                .is_none()
        );
    }
    #[test]
    fn lookup_root_home_by_name() {
        assert!(!lookup_home_by_name(OsStr::new("root")).unwrap().is_empty());
    }
    #[test]
    fn lookup_root_home_by_uid() {
        assert!(!lookup_home_by_uid(0).unwrap().is_empty());
    }
    #[test]
    fn lookup_name_and_uid_agree_for_root() {
        assert_eq!(
            lookup_home_by_name(OsStr::new("root")),
            lookup_home_by_uid(0)
        );
    }
    #[test]
    fn lookup_nonexistent_user_returns_none() {
        assert!(
            lookup_home_by_name(OsStr::new("thisuserdoesnotexist_run0shimtest_xyzzy")).is_none()
        );
    }
    #[test]
    fn lookup_very_high_uid_returns_none() {
        assert!(lookup_home_by_uid(u32::MAX).is_none());
    }
}
