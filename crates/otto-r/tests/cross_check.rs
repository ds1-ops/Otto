//! Runs `cross_check.R`: Otto's R code against established R implementations (TukeyHSD,
//! multcomp, anova.mlm, drc, nls). Skipped when R with Otto's packages is not set up.

use std::path::Path;
use std::process::Command;

#[test]
fn otto_r_code_agrees_with_established_implementations() {
    let status = otto_r::status();
    let Some(rscript) = status.rscript.clone().filter(|_| status.ready()) else {
        eprintln!("skipped: the statistics engine (R with Otto's packages) is not set up");
        return;
    };
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new(rscript)
        .arg("--vanilla")
        .arg(dir.join("tests").join("cross_check.R"))
        .arg(dir.join("r"))
        .arg(otto_r::library_dir())
        .env("R_LIBS", otto_r::library_dir())
        .output()
        .expect("run Rscript");
    let stdout = String::from_utf8_lossy(&out.stdout);
    println!("{stdout}");
    eprintln!("{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success() && stdout.contains("All checks passed"), "cross-checks failed:\n{stdout}");
}
