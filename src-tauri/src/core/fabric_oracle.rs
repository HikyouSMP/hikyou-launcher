//! Executable reference contract; not a second hand-written set of expected results.

use super::*;

#[tokio::test]
#[ignore = "requires an explicitly selected trusted JDK 17+ and downloads verified official Fabric artifacts; no game/accounts"]
async fn official_fabric_predicates_agree() -> Result<(), Box<dyn std::error::Error>> {
    use std::process::Stdio;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let java = std::env::var_os("HIKYOU_TEST_JAVA").ok_or("HIKYOU_TEST_JAVA is required")?;
    let versions = [
        "1",
        "1.0",
        "1.0.0",
        "1.16",
        "1.16.5",
        "1.21.1",
        "26.3",
        "26.3.0",
        "26.3.1",
        "26.30",
        "26.3-alpha.1",
        "26.3-pre.1",
        "26.3-rc.1",
        "26.4-alpha.1",
        "0.5.0",
        "0.5.1",
        "0.6.0",
        "26.3-",
        "26.3-A",
        "26.3-a",
        "26.3-alpha-beta",
        "26.3-01",
        "26.3-70000",
        "26.3-80000",
        "26.3.0.0.1",
    ];
    let predicates = [
        "*",
        "1.x",
        "1.16.x",
        "26.3.x",
        "=26.3.*",
        ">=26.3",
        ">26.3-",
        ">=26.3-",
        "<=26.3-",
        "<26.4-",
        "~26.3",
        "^0.5",
        "~0.5.0",
        ">=1.16 <26.4-",
        "=26.3-",
        ">26.3-A",
        ">26.3-alpha-beta",
        ">26.3-01",
        ">26.3-70000",
        "=26.3.0.0.1",
    ];
    let cases: Vec<_> = versions
        .iter()
        .flat_map(|version| {
            predicates
                .iter()
                .map(move |predicate| (*version, *predicate))
        })
        .collect();
    let mut input = String::new();
    for (version, predicate) in &cases {
        input.push_str(&format!("{version}\t{predicate}\n"));
    }
    for loader in ["0.18.4", "0.19.5"] {
        let root = crate::core::test_support::physical_tempdir();
        let jar = root.path().join("fabric-loader.jar");
        std::fs::write(&jar, loader_bundle::artifact(loader).await?)?;
        let source = root.path().join("FabricPredicateOracle.java");
        std::fs::write(
            &source,
            include_str!("../../../tests/fixtures/FabricPredicateOracle.java"),
        )?;
        let mut command = tokio::process::Command::new(&java);
        command
            .args(["-Xms32M", "-Xmx128M", "-cp"])
            .arg("fabric-loader.jar")
            .arg("FabricPredicateOracle.java")
            .current_dir(root.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command.spawn()?;
        let mut stdin = child.stdin.take().ok_or("oracle stdin missing")?;
        let stdout = child.stdout.take().ok_or("oracle stdout missing")?;
        let stderr = child.stderr.take().ok_or("oracle stderr missing")?;
        let read = |stream: Box<dyn tokio::io::AsyncRead + Unpin + Send>| async move {
            let mut bytes = Vec::new();
            stream.take(256 * 1024 + 1).read_to_end(&mut bytes).await?;
            if bytes.len() > 256 * 1024 {
                return Err(std::io::Error::other("oracle output limit"));
            }
            Ok::<_, std::io::Error>(bytes)
        };
        let (out, err, status) = tokio::time::timeout(std::time::Duration::from_secs(60), async {
            stdin.write_all(input.as_bytes()).await?;
            stdin.shutdown().await?;
            drop(stdin);
            let (out, err) = tokio::try_join!(read(Box::new(stdout)), read(Box::new(stderr)))?;
            Ok::<_, std::io::Error>((out, err, child.wait().await?))
        })
        .await??;
        if !status.success() {
            return Err(
                format!("Fabric reference failed: {}", String::from_utf8_lossy(&err)).into(),
            );
        }
        let out = std::str::from_utf8(&out)?;
        let results: Vec<_> = out.lines().collect();
        assert_eq!(results.len(), cases.len());
        for ((version, predicate), expected) in cases.iter().zip(results) {
            let actual = local_predicate_matches(Some(version), &serde_json::json!(predicate))?;
            assert_eq!(
                actual,
                expected.parse::<bool>()?,
                "Fabric {loader}: {version} / {predicate}"
            );
        }
        eprintln!(
            "Fabric {loader}: {} reference comparisons passed",
            cases.len()
        );
    }
    Ok(())
}
