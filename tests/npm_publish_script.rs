#[cfg(unix)]
mod npm_publish_script_tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use tempfile::tempdir;

    fn create_executable(path: &Path, content: &str) {
        fs::write(path, content).expect("script should be written");
        let mut perms = fs::metadata(path)
            .expect("script metadata should be available")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("script permissions should be set");
    }

    fn prepare_mock_npm(bin_dir: &Path, log_path: &Path) {
        let npm_script = format!(
            "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$*\" > \"{}\"\n",
            log_path.display()
        );
        create_executable(&bin_dir.join("npm"), &npm_script);
    }

    fn prepare_package_dir(root: &Path) -> PathBuf {
        let package_dir = root.join("pkg");
        fs::create_dir_all(&package_dir).expect("package directory should exist");
        fs::write(
            package_dir.join("package.json"),
            "{\"name\":\"@launch-code/cli\",\"version\":\"0.1.0\"}\n",
        )
        .expect("package.json should be written");
        package_dir
    }

    fn run_publish_script(
        package_dir: &Path,
        bin_dir: &Path,
        lcode_registry: Option<&str>,
        npm_registry: Option<&str>,
    ) -> std::process::Output {
        let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/npm_publish.sh");
        let path_env = std::env::var("PATH").unwrap_or_default();
        let mut cmd = Command::new("bash");
        cmd.arg(script_path)
            .arg("--package-dir")
            .arg(package_dir)
            .env("PATH", format!("{}:{}", bin_dir.display(), path_env));

        if let Some(value) = lcode_registry {
            cmd.env("LCODE_NPM_PUBLISH_REGISTRY", value);
        } else {
            cmd.env_remove("LCODE_NPM_PUBLISH_REGISTRY");
        }

        if let Some(value) = npm_registry {
            cmd.env("NPM_CONFIG_REGISTRY", value);
        } else {
            cmd.env_remove("NPM_CONFIG_REGISTRY");
        }

        cmd.output().expect("publish script should run")
    }

    #[test]
    fn publish_uses_lcode_registry_env() {
        let tmp = tempdir().expect("temp dir should exist");
        let package_dir = prepare_package_dir(tmp.path());
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).expect("bin directory should exist");
        let log_path = tmp.path().join("npm.log");
        prepare_mock_npm(&bin_dir, &log_path);

        let output = run_publish_script(
            &package_dir,
            &bin_dir,
            Some("https://registry.local.example"),
            None,
        );

        assert!(
            output.status.success(),
            "publish script should succeed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let args = fs::read_to_string(&log_path).expect("npm log should be readable");
        assert!(args.contains("publish"), "npm publish should be called");
        assert!(
            args.contains("--registry https://registry.local.example"),
            "LCODE_NPM_PUBLISH_REGISTRY should be forwarded to npm publish"
        );
    }

    #[test]
    fn publish_prefers_lcode_registry_over_npm_config_registry() {
        let tmp = tempdir().expect("temp dir should exist");
        let package_dir = prepare_package_dir(tmp.path());
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).expect("bin directory should exist");
        let log_path = tmp.path().join("npm.log");
        prepare_mock_npm(&bin_dir, &log_path);

        let output = run_publish_script(
            &package_dir,
            &bin_dir,
            Some("https://registry.lcode.example"),
            Some("https://registry.npm-config.example"),
        );

        assert!(
            output.status.success(),
            "publish script should succeed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let args = fs::read_to_string(&log_path).expect("npm log should be readable");
        assert!(
            args.contains("--registry https://registry.lcode.example"),
            "LCODE_NPM_PUBLISH_REGISTRY should override NPM_CONFIG_REGISTRY"
        );
        assert!(
            !args.contains("registry.npm-config.example"),
            "fallback NPM_CONFIG_REGISTRY should not be used when LCODE_NPM_PUBLISH_REGISTRY is set"
        );
    }

    #[test]
    fn publish_rejects_invalid_access_value() {
        let tmp = tempdir().expect("temp dir should exist");
        let package_dir = prepare_package_dir(tmp.path());
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).expect("bin directory should exist");
        let log_path = tmp.path().join("npm.log");
        prepare_mock_npm(&bin_dir, &log_path);

        let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/npm_publish.sh");
        let path_env = std::env::var("PATH").unwrap_or_default();
        let output = Command::new("bash")
            .arg(script_path)
            .arg("--package-dir")
            .arg(&package_dir)
            .arg("--access")
            .arg("internal")
            .env("PATH", format!("{}:{}", bin_dir.display(), path_env))
            .output()
            .expect("publish script should run");

        assert!(
            !output.status.success(),
            "publish script should fail for invalid access mode"
        );

        let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
        assert!(
            stderr.contains("Invalid --access value"),
            "stderr should explain access validation"
        );
    }
}
