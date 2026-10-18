use super::*;

fn args(items: &[&str]) -> Vec<OsString> {
    items.iter().map(OsString::from).collect()
}

fn parse(items: &[&str]) -> Result<Profile, String> {
    Profile::from_args(args(items), "--send")
}

#[test]
fn without_the_flag_the_product_profile_is_used() {
    assert_eq!(parse(&[]), Ok(Profile::default()));
    assert_eq!(parse(&["--autostart"]), Ok(Profile::default()));
    assert!(!Profile::default().testing());
}

#[test]
fn flag_selects_the_test_folder() {
    let profile = parse(&["--test-profile", r"C:\Temp\run 1\профиль", "--autostart"]).expect("profile");
    assert!(profile.testing());
    assert_eq!(profile.local_data(), PathBuf::from(r"C:\Temp\run 1\профиль"));
    assert_eq!(profile.temp_dir(), PathBuf::from(r"C:\Temp\run 1\профиль\Temp"));
}

#[test]
fn trailing_separators_are_dropped_so_quoting_stays_valid() {
    let profile = parse(&["--test-profile", r"C:\Temp\run\"]).expect("profile");
    assert_eq!(profile.local_data(), PathBuf::from(r"C:\Temp\run"));
    assert_eq!(Profile::test(PathBuf::from(r"C:\")).local_data(), PathBuf::from(r"C:\"));
}

#[test]
fn flag_needs_an_absolute_folder_before_sent_paths() {
    assert!(parse(&["--test-profile"]).is_err());
    assert!(parse(&["--test-profile", ""]).is_err());
    assert!(parse(&["--test-profile", "relative"]).is_err());
    assert!(parse(&["--test-profile", "--send", r"C:\a.txt"]).is_err());
    assert_eq!(parse(&["--send", "--test-profile", r"C:\a"]), Ok(Profile::default()));
}

#[test]
fn product_profile_keeps_the_released_names() {
    let profile = Profile::default();
    assert_eq!(profile.name(), "Traydex");
    assert_eq!(profile.mutex(), r"Local\Traydex.Singleton");
    assert_eq!(profile.relay_class(), "Traydex.Relay");
    assert_eq!(profile.registry_name(), "Traydex");
    assert_eq!(profile.autostart_name(), "Traydex");
    assert_eq!(profile.command(r"C:\App\traydex.exe", "--autostart"), r#""C:\App\traydex.exe" --autostart"#);
}

#[test]
fn test_profile_uses_its_own_names_everywhere() {
    let product = Profile::default();
    let test = Profile::test(PathBuf::from(r"C:\Temp\e2e"));
    assert_ne!(test.name(), product.name());
    assert_ne!(test.mutex(), product.mutex());
    assert_ne!(test.relay_class(), product.relay_class());
    assert_ne!(test.registry_name(), product.registry_name());
    assert_ne!(test.autostart_name(), product.autostart_name());
    assert!(test.mutex().starts_with(r"Local\"));
}

#[test]
fn test_commands_carry_the_profile_before_other_arguments() {
    let test = Profile::test(PathBuf::from(r"C:\Temp\e2e run"));
    let command = test.command(r"C:\Temp\bin\traydex.exe", r#"--send "%1""#);
    assert_eq!(command, r#""C:\Temp\bin\traydex.exe" --test-profile "C:\Temp\e2e run" --send "%1""#);
}

#[test]
fn marker_beside_the_exe_keeps_data_in_its_folder() {
    let sandbox = crate::testing::Sandbox::new();
    let folder = sandbox.folder("Мои программы/Traydex portable");
    let exe = folder.join("Traydex.exe");
    assert_eq!(Profile::portable_beside(&exe), None, "no marker, no portable mode");
    sandbox.file("Мои программы/Traydex portable/portable.txt", "");
    let portable = Profile::portable_beside(&exe).expect("portable profile");
    assert_eq!(portable.data_root(), folder.join("data"));
    assert!(!portable.testing());
    assert_eq!(portable.registry_name(), Profile::default().registry_name());
    assert_eq!(portable.command(r"C:\App\Traydex.exe", "--autostart"), r#""C:\App\Traydex.exe" --autostart"#);
}

#[test]
fn installed_and_test_profiles_keep_data_in_a_traydex_folder() {
    let test = Profile::test(PathBuf::from(r"C:\Temp\e2e"));
    assert_eq!(test.data_root(), PathBuf::from(r"C:\Temp\e2e\Traydex"));
}
