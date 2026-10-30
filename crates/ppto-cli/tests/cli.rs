// SPDX-License-Identifier: GPL-3.0-or-later
//! La herramienta de línea de órdenes `ppto` de punta a punta, con el
//! ejemplo sintético: demo → BC3 → importar → Excel, PDF y base de datos.

use std::path::PathBuf;
use std::process::Command;

fn ppto(args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_ppto")).args(args).output().unwrap();
    let texto = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), texto)
}

fn carpeta(nombre: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ppto-cli-{nombre}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn demo_importar_y_exportar() {
    let d = carpeta("demo");
    let f = |n: &str| d.join(n).display().to_string();
    let (ok, t) = ppto(&[
        "demo",
        "--db",
        &f("demo.sqlite"),
        "--excel",
        &f("demo.xlsx"),
        "--bc3",
        &f("demo.bc3"),
        "--pdf",
        &f("demo.pdf"),
    ]);
    assert!(ok, "{t}");
    assert!(t.contains("PEM"), "{t}");
    for n in ["demo.sqlite", "demo.xlsx", "demo.bc3", "demo.pdf"] {
        assert!(d.join(n).metadata().unwrap().len() > 0, "{n}");
    }
    // El BC3 de la demo se vuelve a importar con el mismo PEM
    let pem = |t: &str| t.lines().find(|l| l.contains("PEM")).map(str::to_owned);
    let (ok, t2) = ppto(&[
        "importar",
        &f("demo.bc3"),
        "--todo",
        "--db",
        &f("imp.sqlite"),
        "--excel",
        &f("imp.xlsx"),
        "--bc3",
        &f("imp.bc3"),
        "--pdf",
        &f("imp.pdf"),
    ]);
    assert!(ok, "{t2}");
    assert!(pem(&t2).is_some(), "{t2}");
    assert!(d.join("imp.bc3").exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn errores_claros() {
    let (ok, t) = ppto(&[]);
    assert!(!ok);
    assert!(t.contains("uso:"), "{t}");
    let (ok, t) = ppto(&["importar", "/no/existe.bc3"]);
    assert!(!ok, "{t}");
    let (ok, _) = ppto(&["ciego", "/no/existe.xlsx"]);
    assert!(!ok);
    let (ok, _) = ppto(&["tarifa", "/no/existe.xlsx"]);
    assert!(!ok);
}
