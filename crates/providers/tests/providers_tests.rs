//! Provider tests with fixtures. No test depends on the host machine's
//! installed software: macOS discovery scans fixture directories, Homebrew
//! and CLI providers read fixture command output through a fake runner.

use assetmesh_core::domain::software::{InstallSource, SoftwareCategory};
use assetmesh_core::ports::providers::SoftwareCandidate;
use assetmesh_core::ports::providers::SoftwareDiscoveryProvider;
use assetmesh_core::AppError;
use assetmesh_providers::CommandError;
use assetmesh_providers::{CliToolsProvider, HomebrewProvider, MacosApplicationsProvider};
use std::io::Write;
use std::path::PathBuf;
use std::process::Output;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "assetmesh-providers-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

type RecordedCommands = std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<String>)>>>;

/// Fake command runner with canned outputs; records the commands it ran
/// through a shared handle the test keeps.
/// Canned responses keyed by program; the bool is the exit status. Programs
/// absent from the map simulate `CommandError::NotFound`.
#[derive(Debug, Clone)]
struct FakeRunner {
    responses: std::collections::HashMap<String, (String, String, bool)>,
    ran: RecordedCommands,
}

impl FakeRunner {
    fn new() -> Self {
        FakeRunner {
            responses: Default::default(),
            ran: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    fn recorded(&self) -> Vec<(String, Vec<String>)> {
        self.ran.lock().unwrap().clone()
    }

    fn respond(mut self, program: &str, stdout: &str, success: bool) -> Self {
        self.responses.insert(
            program.to_string(),
            (stdout.to_string(), String::new(), success),
        );
        self
    }
}

fn make_output(stdout: &str, stderr: &str, success: bool) -> Output {
    let mut status = std::process::Command::new("true").output().unwrap().status;
    if !success {
        // Force a failing status without launching a shell-dependent program.
        status = std::process::Command::new("false").output().unwrap().status;
    }
    Output {
        status,
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

impl assetmesh_providers::CommandRunner for FakeRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output, CommandError> {
        self.ran.lock().unwrap().push((
            program.to_string(),
            args.iter().map(|s| s.to_string()).collect(),
        ));
        match self.responses.get(program) {
            Some((stdout, stderr, success)) => Ok(make_output(stdout, stderr, *success)),
            // Unlisted programs simulate tools that are not installed.
            None => Err(CommandError::NotFound),
        }
    }
}

/// Writes a fixture .app bundle and returns its path.
fn write_app(root: &std::path::Path, name: &str, info_plist: &str) -> PathBuf {
    let bundle = root.join(format!("{name}.app"));
    std::fs::create_dir_all(bundle.join("Contents")).unwrap();
    let mut file = std::fs::File::create(bundle.join("Contents").join("Info.plist")).unwrap();
    file.write_all(info_plist.as_bytes()).unwrap();
    bundle
}

const APP_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
    <key>CFBundleIdentifier</key><string>com.example.FixtureApp</string>
    <key>CFBundleShortVersionString</key><string>2.5.0</string>
    <key>CFBundleExecutable</key><string>fixture-bin</string>
</dict></plist>"#;

#[test]
fn macos_provider_normalizes_app_bundles() {
    let root = temp_dir("normalize");
    write_app(&root, "FixtureApp", APP_PLIST);

    let provider = MacosApplicationsProvider::with_roots(vec![root.clone()]);
    let candidates = provider.scan().unwrap();
    assert_eq!(candidates.len(), 1);
    let app = &candidates[0];
    assert_eq!(app.provider, "macos_applications");
    assert_eq!(app.display_name, "FixtureApp");
    assert_eq!(app.category, SoftwareCategory::Application);
    assert_eq!(app.install_source, InstallSource::MacosApp);
    assert_eq!(app.version.as_deref(), Some("2.5.0"));
    assert_eq!(
        app.external_refs,
        vec![assetmesh_core::ports::providers::CandidateRef::new(
            "bundle_id",
            "com.example.FixtureApp"
        )
        .unwrap()]
    );
    assert_eq!(
        app.executable_path.as_deref(),
        Some(
            root.join("FixtureApp.app")
                .join("Contents")
                .join("MacOS")
                .join("fixture-bin")
                .to_str()
                .unwrap()
        )
    );
    assert!(app
        .install_location
        .as_deref()
        .unwrap()
        .ends_with("FixtureApp.app"));
}

#[test]
fn macos_provider_skips_malformed_and_missing_metadata() {
    let root = temp_dir("malformed");
    write_app(&root, "GoodApp", APP_PLIST);
    // Missing Info.plist.
    std::fs::create_dir_all(root.join("NoPlist.app").join("Contents")).unwrap();
    // Malformed plist content.
    write_app(&root, "BadApp", "this is not a plist");
    // A .app bundle whose plist is not a dictionary.
    write_app(
        &root,
        "NotADict",
        r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><array/></plist>"#,
    );

    let provider = MacosApplicationsProvider::with_roots(vec![root.clone()]);
    let candidates = provider.scan().unwrap();
    assert_eq!(candidates.len(), 1, "only the good app is discovered");
    assert_eq!(candidates[0].display_name, "GoodApp");

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn macos_provider_dedupes_bundle_ids_across_roots() {
    let root1 = temp_dir("dedupe1");
    let root2 = temp_dir("dedupe2");
    write_app(&root1, "FixtureApp", APP_PLIST);
    write_app(&root2, "FixtureApp", APP_PLIST);

    let provider = MacosApplicationsProvider::with_roots(vec![root1, root2]);
    let candidates = provider.scan().unwrap();
    assert_eq!(candidates.len(), 1, "same bundle id in two roots dedupes");
}

#[test]
fn macos_provider_tolerates_missing_root() {
    let provider =
        MacosApplicationsProvider::with_roots(vec![PathBuf::from("/nonexistent-assetmesh-root")]);
    assert_eq!(provider.scan().unwrap(), Vec::<SoftwareCandidate>::new());
}

#[test]
fn homebrew_provider_parses_formula_and_cask_output() {
    let stdout = r#"{
      "formulae": [{
        "name": "ripgrep",
        "desc": "Search tool like grep and The Silver Searcher",
        "versions": {"stable": "14.1.1"},
        "installed": [{"version": "14.1.0"}]
      }],
      "casks": [{
        "token": "visual-studio-code",
        "name": ["Microsoft Visual Studio Code"],
        "version": "1.93.0",
        "desc": "Code editor"
      }]
    }"#;
    let runner = FakeRunner::new().respond("brew", stdout, true);
    let provider = HomebrewProvider::with_runner("brew", runner);
    let candidates = provider.scan().unwrap();

    assert_eq!(candidates.len(), 2);
    let rg = candidates
        .iter()
        .find(|c| c.display_name == "ripgrep")
        .unwrap();
    assert_eq!(rg.category, SoftwareCategory::Package);
    assert_eq!(rg.install_source, InstallSource::HomebrewFormula);
    // Installed version preferred over tracked stable.
    assert_eq!(rg.version.as_deref(), Some("14.1.0"));
    assert_eq!(rg.external_refs[0].key(), ("homebrew_formula", "ripgrep"));

    let code = candidates
        .iter()
        .find(|c| c.display_name == "Microsoft Visual Studio Code")
        .unwrap();
    assert_eq!(code.category, SoftwareCategory::Application);
    assert_eq!(code.install_source, InstallSource::HomebrewCask);
    assert_eq!(code.version.as_deref(), Some("1.93.0"));
    assert_eq!(
        code.external_refs[0].key(),
        ("homebrew_cask", "visual-studio-code")
    );
}

#[test]
fn homebrew_provider_handles_partial_fields() {
    let stdout = r#"{
      "formulae": [{
        "name": "openssl@3",
        "installed": [],
        "versions": {}
      }],
      "casks": [{"token": "weird cask with space"}]
    }"#;
    let runner = FakeRunner::new().respond("brew", stdout, true);
    let provider = HomebrewProvider::with_runner("brew", runner);
    let candidates = provider.scan().unwrap();

    // openssl@3: no installed/stable version → version absent, still found.
    let openssl = candidates
        .iter()
        .find(|c| c.display_name == "openssl@3")
        .unwrap();
    assert_eq!(openssl.version, None);

    // A cask token with whitespace cannot become a deterministic external
    // ref and is skipped rather than normalized into bad canonical data.
    assert_eq!(candidates.len(), 1);
}

#[test]
fn homebrew_unavailable_and_failures_are_typed_errors() {
    // Command not found.
    let provider = HomebrewProvider::with_runner("brew", FakeRunner::new());
    let err = provider.scan().unwrap_err();
    assert!(matches!(err, AppError::ProviderUnavailable { .. }), "{err}");

    // Command fails.
    let runner = FakeRunner::new().respond("brew", "", false);
    let provider = HomebrewProvider::with_runner("brew", runner);
    let err = provider.scan().unwrap_err();
    assert!(matches!(err, AppError::ProviderUnavailable { .. }), "{err}");

    // Malformed JSON.
    let runner = FakeRunner::new().respond("brew", "not json", true);
    let provider = HomebrewProvider::with_runner("brew", runner);
    let err = provider.scan().unwrap_err();
    assert!(err.to_string().contains("not valid JSON"), "{err}");

    // The provider never touches canonical data on failure: it returns an
    // error or empty output, and never partial state.
}

#[test]
fn homebrew_runs_read_only_commands_only() {
    let stdout = r#"{"formulae": [], "casks": []}"#;
    let runner = FakeRunner::new().respond("brew", stdout, true);
    let provider = HomebrewProvider::with_runner("brew", runner.clone());
    provider.scan().unwrap();

    let ran = runner.recorded();
    assert_eq!(ran.len(), 1);
    assert_eq!(ran[0].0, "brew");
    assert_eq!(
        ran[0].1.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        vec!["info", "--json=v2", "--installed"]
    );
}

#[test]
fn cli_provider_parses_npm_and_pipx_outputs() {
    let npm_stdout = r#"{
      "dependencies": {
        "typescript": {"version": "5.6.0"},
        "@angular/cli": {"version": "18.0.0"}
      }
    }"#;
    let pipx_stdout = r#"{
      "venvs": {
        "black": {"metadata": {"main_package": {"package": "black", "package_version": "24.8.0"}}},
        "ruff": {"metadata": {"main_package": {"package": "ruff", "package_version": "0.6.0"}}}
      }
    }"#;
    let runner =
        FakeRunner::new()
            .respond("npm", npm_stdout, true)
            .respond("pipx", pipx_stdout, true);
    let provider = CliToolsProvider::with_runner(runner);
    let candidates = provider.scan().unwrap();

    assert_eq!(candidates.len(), 4);
    let ts = candidates
        .iter()
        .find(|c| c.display_name == "typescript")
        .unwrap();
    assert_eq!(ts.install_source, InstallSource::NpmGlobal);
    assert_eq!(ts.category, SoftwareCategory::Cli);
    assert_eq!(ts.version.as_deref(), Some("5.6.0"));
    assert_eq!(ts.external_refs[0].key(), ("npm", "typescript"));

    let black = candidates
        .iter()
        .find(|c| c.display_name == "black")
        .unwrap();
    assert_eq!(black.install_source, InstallSource::Pipx);
    assert_eq!(black.version.as_deref(), Some("24.8.0"));
    assert_eq!(black.external_refs[0].key(), ("pipx", "black"));
}

#[test]
fn cli_provider_skips_missing_tools_and_reports_failures() {
    // Neither npm nor pipx installed: no candidates, no error.
    let provider = CliToolsProvider::with_runner(FakeRunner::new());
    assert_eq!(provider.scan().unwrap(), Vec::<SoftwareCandidate>::new());

    // npm present and failing: typed error (must not look like "no tools").
    let runner = FakeRunner::new().respond("npm", "", false);
    let provider = CliToolsProvider::with_runner(runner);
    let err = provider.scan().unwrap_err();
    assert!(matches!(err, AppError::ProviderUnavailable { .. }), "{err}");

    // Malformed npm output: typed error.
    let runner = FakeRunner::new().respond("npm", "}", true);
    let provider = CliToolsProvider::with_runner(runner);
    let err = provider.scan().unwrap_err();
    assert!(err.to_string().contains("not valid JSON"), "{err}");
}

#[test]
fn macos_provider_skips_apps_with_unusable_bundle_ids() {
    let root = temp_dir("bad-bundle-id");
    write_app(&root, "GoodApp", APP_PLIST);
    // A bundle identifier containing whitespace cannot become a
    // deterministic external ref; the app is skipped, not fatal.
    write_app(
        &root,
        "BadIdApp",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
    <key>CFBundleIdentifier</key><string>com.example.Bad Id</string>
</dict></plist>"#,
    );

    let provider = MacosApplicationsProvider::with_roots(vec![root.clone()]);
    let candidates = provider.scan().unwrap();
    assert_eq!(
        candidates.len(),
        1,
        "only the well-formed app is discovered"
    );
    assert_eq!(candidates[0].display_name, "GoodApp");

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn missing_tool_is_a_typed_not_found_not_a_string_match() {
    // The seam reports absence via CommandError::NotFound; the CLI provider
    // skips such tools without manufacturing an error.
    let runner = FakeRunner::new();
    let provider = CliToolsProvider::with_runner(runner.clone());
    assert_eq!(provider.scan().unwrap(), Vec::<SoftwareCandidate>::new());
    // Both sub-sources were still attempted through the seam.
    assert_eq!(runner.recorded().len(), 2);
}
