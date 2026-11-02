use video_kadr_backend::config::AppConfig;

#[test]
fn server_configuration_admits_only_complete_absolute_person_runtime() {
    if let Ok(mode) = std::env::var("VIDEO_KADR_PERSON_CONFIG_CASE") {
        let result = AppConfig::from_env();
        match mode.as_str() {
            "absent" => assert!(result.unwrap().person.is_none()),
            "complete" => {
                let config = result.unwrap().person.unwrap();
                assert_eq!(config.executable, std::path::Path::new("/person/native"));
                assert_eq!(config.model, std::path::Path::new("/person/model"));
                assert_eq!(config.runtime_library, std::path::Path::new("/person/ort"));
            }
            "partial" | "relative" => assert!(result.is_err()),
            _ => panic!("unknown configuration fixture"),
        }
        return;
    }
    for mode in ["absent", "complete", "partial", "relative"] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .env_clear()
            .arg("--exact")
            .arg("server_configuration_admits_only_complete_absolute_person_runtime")
            .arg("--nocapture")
            .env("VIDEO_KADR_PERSON_CONFIG_CASE", mode);
        if mode != "absent" {
            command.env("PERSON_BIN", "/person/native");
        }
        if mode == "complete" || mode == "relative" {
            command
                .env(
                    "PERSON_MODEL",
                    if mode == "relative" {
                        "relative"
                    } else {
                        "/person/model"
                    },
                )
                .env("PERSON_ORT_LIBRARY", "/person/ort");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{mode}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
