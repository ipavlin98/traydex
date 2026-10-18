use super::*;
use std::path::PathBuf;

#[test]
fn product_entries_keep_released_keys_and_commands() {
    let profile = Profile::default();
    assert_eq!(
        menus(&profile),
        [r"Software\Classes\*\shell\Traydex".to_string(), r"Software\Classes\Directory\shell\Traydex".to_string()]
    );
    assert_eq!(autostart_command(&profile, r"C:\App\traydex.exe"), r#""C:\App\traydex.exe" --autostart"#);
    assert_eq!(menu_command(&profile, r"C:\App\traydex.exe"), r#""C:\App\traydex.exe" --send "%1""#);
}

#[test]
fn test_entries_use_separate_keys_and_start_the_test_profile() {
    let profile = Profile::test(PathBuf::from(r"C:\Temp\Тест 1"));
    assert_eq!(
        menus(&profile),
        [
            r"Software\Classes\*\shell\TraydexTest".to_string(),
            r"Software\Classes\Directory\shell\TraydexTest".to_string()
        ]
    );
    assert_eq!(
        autostart_command(&profile, r"C:\Temp\bin\traydex.exe"),
        r#""C:\Temp\bin\traydex.exe" --test-profile "C:\Temp\Тест 1" --autostart"#
    );
    assert_eq!(
        menu_command(&profile, r"C:\Temp\bin\traydex.exe"),
        r#""C:\Temp\bin\traydex.exe" --test-profile "C:\Temp\Тест 1" --send "%1""#
    );
}
