include!("app_commands.rs");

macro_rules! command_policy {
    ($($module:ident::$command:ident [$($role:ident),*]),* $(,)?) => {
        &[$((stringify!($command), &[$(stringify!($role)),*] as &[&str])),*]
    };
}

macro_rules! command_names {
    ($($module:ident::$command:ident [$($role:ident),*]),* $(,)?) => {
        &[$(stringify!($command)),*]
    };
}

const POLICY: &[(&str, &[&str])] = application_commands!(command_policy);
const COMMANDS: &[&str] = application_commands!(command_names);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Integration harnesses do not inherit the app's resource. The lib
        // unit-test harness links Tauri's resource explicitly in lib.rs.
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-tests=/MANIFESTINPUT:common-controls.manifest");
        println!("cargo:rerun-if-changed=common-controls.manifest");
    }
    println!("cargo:rerun-if-changed=app_commands.rs");
    let mut permissions = String::new();
    for role in ["main", "game_log"] {
        let names = POLICY
            .iter()
            .filter(|(_, roles)| role == "main" || roles.contains(&role))
            .map(|(name, _)| format!("{name:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        permissions.push_str(&format!(
            "[[permission]]\nidentifier = \"app-{}\"\ncommands.allow = [{names}]\n\n",
            role.replace('_', "-")
        ));
    }
    let path = std::path::Path::new("permissions/generated-policy.toml");
    let parent = path.parent().ok_or("permission directory is missing")?;
    std::fs::create_dir_all(parent)?;
    if std::fs::read_to_string(path).ok().as_deref() != Some(&permissions) {
        std::fs::write(path, permissions)?;
    }
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )?;
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Tauri links this resource to application binaries. Unit-test
        // harnesses explicitly link the same resource from lib.rs instead.
        println!(
            "cargo:rustc-link-search=native={}",
            std::env::var("OUT_DIR")?
        );
    }
    Ok(())
}
