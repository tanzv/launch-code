#[cfg(unix)]
mod bpmn_release_scripts_tests {
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
            "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s|%s\\n' \"$PWD\" \"$*\" >> \"{}\"\n",
            log_path.display()
        );
        create_executable(&bin_dir.join("npm"), &npm_script);
    }

    fn prepare_mock_git(bin_dir: &Path) {
        let git_script = "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"${1:-}\" == \"-C\" ]]; then\n  shift 2\nfi\nif [[ \"${1:-}\" == \"status\" ]]; then\n  exit 0\nfi\nexit 0\n";
        create_executable(&bin_dir.join("git"), git_script);
    }

    fn create_package_dir(root: &Path) -> PathBuf {
        let package_dir = root.join("package");
        fs::create_dir_all(&package_dir).expect("package directory should exist");
        fs::write(
            package_dir.join("package.json"),
            "{\"name\":\"launch-code\",\"version\":\"0.1.0\"}\n",
        )
        .expect("package.json should be written");
        package_dir
    }

    #[test]
    fn prepare_script_copies_bpmn_into_package() {
        let tmp = tempdir().expect("temp dir should exist");
        let package_dir = create_package_dir(tmp.path());
        let source_file = tmp.path().join("release-flow.bpmn");
        fs::write(&source_file, "<bpmn:definitions id=\"test\" />\n")
            .expect("source bpmn should be written");

        let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("scripts/prepare_bpmn_package.sh");
        let output = Command::new("bash")
            .arg(script_path)
            .arg("--package-dir")
            .arg(&package_dir)
            .arg("--source-file")
            .arg(&source_file)
            .output()
            .expect("prepare script should run");

        assert!(
            output.status.success(),
            "prepare script should succeed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let packaged_bpmn = package_dir.join("bpmn/npm-publish.bpmn");
        assert!(packaged_bpmn.exists(), "packaged bpmn should exist");

        let content = fs::read_to_string(packaged_bpmn).expect("packaged bpmn should be readable");
        assert!(content.contains("id=\"test\""), "packaged bpmn should match source");
    }

    #[test]
    fn prepare_script_fails_when_source_file_missing() {
        let tmp = tempdir().expect("temp dir should exist");
        let package_dir = create_package_dir(tmp.path());
        let missing_source = tmp.path().join("missing-flow.bpmn");

        let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("scripts/prepare_bpmn_package.sh");
        let output = Command::new("bash")
            .arg(script_path)
            .arg("--package-dir")
            .arg(&package_dir)
            .arg("--source-file")
            .arg(&missing_source)
            .output()
            .expect("prepare script should run");

        assert!(
            !output.status.success(),
            "prepare script should fail when source file is missing"
        );

        let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
        assert!(
            stderr.contains("BPMN source file does not exist"),
            "stderr should describe missing source file"
        );
    }

    #[test]
    fn release_script_runs_version_pack_and_publish_with_registry() {
        let tmp = tempdir().expect("temp dir should exist");
        let package_dir = create_package_dir(tmp.path());
        let source_file = tmp.path().join("release-flow.bpmn");
        fs::write(&source_file, "<bpmn:definitions id=\"release\" />\n")
            .expect("source bpmn should be written");

        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).expect("bin directory should exist");

        let npm_log = tmp.path().join("npm.log");
        prepare_mock_npm(&bin_dir, &npm_log);
        prepare_mock_git(&bin_dir);

        let script_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/release_bpmn_package.sh");
        let path_env = std::env::var("PATH").unwrap_or_default();
        let output = Command::new("bash")
            .arg(script_path)
            .arg("--package-dir")
            .arg(&package_dir)
            .arg("--source-file")
            .arg(&source_file)
            .arg("--version")
            .arg("0.2.0")
            .arg("--tag")
            .arg("next")
            .arg("--dry-run")
            .arg("--skip-verification")
            .arg("--allow-dirty")
            .env("PATH", format!("{}:{}", bin_dir.display(), path_env))
            .env("LCODE_NPM_PUBLISH_REGISTRY", "https://registry.local.example")
            .output()
            .expect("release script should run");

        assert!(
            output.status.success(),
            "release script should succeed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let packaged_bpmn = package_dir.join("bpmn/npm-publish.bpmn");
        assert!(packaged_bpmn.exists(), "packaged bpmn should exist after release");

        let log = fs::read_to_string(&npm_log).expect("npm log should be readable");
        assert!(
            log.contains("version 0.2.0 --no-git-tag-version"),
            "release script should bump package version"
        );
        assert!(
            log.contains("pack --dry-run"),
            "release script should run npm pack dry-run"
        );
        assert!(
            log.contains("publish --tag next --registry https://registry.local.example --dry-run"),
            "release script should publish with expected registry and tag"
        );

        let package_json =
            fs::read_to_string(package_dir.join("package.json")).expect("package.json should exist");
        assert!(
            package_json.contains("\"version\":\"0.1.0\""),
            "dry-run release should restore package.json version"
        );
    }

    #[test]
    fn release_script_rejects_invalid_semver() {
        let tmp = tempdir().expect("temp dir should exist");
        let package_dir = create_package_dir(tmp.path());
        let source_file = tmp.path().join("release-flow.bpmn");
        fs::write(&source_file, "<bpmn:definitions id=\"release\" />\n")
            .expect("source bpmn should be written");

        let script_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/release_bpmn_package.sh");
        let output = Command::new("bash")
            .arg(script_path)
            .arg("--package-dir")
            .arg(&package_dir)
            .arg("--source-file")
            .arg(&source_file)
            .arg("--version")
            .arg("invalid-version")
            .arg("--skip-verification")
            .arg("--allow-dirty")
            .output()
            .expect("release script should run");

        assert!(
            !output.status.success(),
            "release script should fail for invalid semver"
        );

        let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
        assert!(
            stderr.contains("Invalid --version value"),
            "stderr should explain semver validation"
        );
    }

    #[test]
    fn release_script_uses_env_file_for_publish_defaults() {
        let tmp = tempdir().expect("temp dir should exist");
        let package_dir = create_package_dir(tmp.path());
        let source_file = tmp.path().join("release-flow.bpmn");
        fs::write(&source_file, "<bpmn:definitions id=\"release\" />\n")
            .expect("source bpmn should be written");

        let env_file = tmp.path().join(".release.env");
        fs::write(
            &env_file,
            "LCODE_NPM_PUBLISH_REGISTRY=https://registry.env-file.example\nLCODE_NPM_PUBLISH_TAG=beta\nLCODE_NPM_PUBLISH_ACCESS=public\nLCODE_NPM_PUBLISH_DRY_RUN=true\n",
        )
        .expect("env file should be written");

        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).expect("bin directory should exist");
        let npm_log = tmp.path().join("npm.log");
        prepare_mock_npm(&bin_dir, &npm_log);
        prepare_mock_git(&bin_dir);

        let script_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/release_bpmn_package.sh");
        let path_env = std::env::var("PATH").unwrap_or_default();
        let output = Command::new("bash")
            .arg(script_path)
            .arg("--package-dir")
            .arg(&package_dir)
            .arg("--source-file")
            .arg(&source_file)
            .arg("--env-file")
            .arg(&env_file)
            .arg("--skip-verification")
            .arg("--allow-dirty")
            .env("PATH", format!("{}:{}", bin_dir.display(), path_env))
            .output()
            .expect("release script should run");

        assert!(
            output.status.success(),
            "release script should succeed with env file defaults: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let log = fs::read_to_string(&npm_log).expect("npm log should be readable");
        assert!(
            log.contains("publish --tag beta --registry https://registry.env-file.example --access public --dry-run"),
            "publish command should inherit tag/registry/access/dry-run from env file"
        );
    }

    #[test]
    fn release_script_rejects_invalid_env_file_line() {
        let tmp = tempdir().expect("temp dir should exist");
        let package_dir = create_package_dir(tmp.path());
        let source_file = tmp.path().join("release-flow.bpmn");
        fs::write(&source_file, "<bpmn:definitions id=\"release\" />\n")
            .expect("source bpmn should be written");

        let env_file = tmp.path().join(".release.env");
        fs::write(&env_file, "BROKEN_LINE\n").expect("env file should be written");

        let script_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/release_bpmn_package.sh");
        let output = Command::new("bash")
            .arg(script_path)
            .arg("--package-dir")
            .arg(&package_dir)
            .arg("--source-file")
            .arg(&source_file)
            .arg("--env-file")
            .arg(&env_file)
            .arg("--skip-verification")
            .arg("--allow-dirty")
            .output()
            .expect("release script should run");

        assert!(
            !output.status.success(),
            "release script should fail when env file contains invalid line"
        );

        let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
        assert!(
            stderr.contains("Invalid env file line"),
            "stderr should describe env file parsing failure"
        );
    }
}
