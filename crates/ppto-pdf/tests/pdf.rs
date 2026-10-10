// SPDX-License-Identifier: GPL-3.0-or-later
//! PDF del presupuesto: datos que recibe la plantilla (cifras calculadas a
//! mano) y generación completa con Typst.

use ppto_core::ejemplos::suelo_radiante;
use ppto_pdf::{OpcionesPdf, datos, fecha_larga, pdf};
use rust_decimal_macros::dec;

fn opciones() -> OpcionesPdf {
    OpcionesPdf {
        cliente: "Cliente de ejemplo".into(),
        referencia: "2026OBM000".into(),
        revision: "R1".into(),
        fecha: fecha_larga(10, 10, 2026),
        lugar: "Madrid".into(),
        empresa: "Ingeniería Ejemplo, S.L.".into(),
        empresa_datos: "CIF B00000000 · Calle Ejemplo 1, Madrid".into(),
        autor: "Ingeniero Ejemplo".into(),
        autor_datos: "Ingeniero Técnico Industrial · Col. n.º 00000".into(),
        logo: Some(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/datos/logo-sintetico.svg").into()),
        gastos_generales: dec!(13),
        beneficio_industrial: dec!(6),
        ..OpcionesPdf::default()
    }
}

#[test]
fn datos_de_la_plantilla() {
    // PEM 7.005,15; GG 13 % 910,67; BI 6 % 420,31; base 8.336,13; IVA 21 % 1.750,59; total 10.086,72
    let d = datos(&suelo_radiante(), &opciones(), Some("logo.svg".into())).unwrap();
    assert_eq!(d["fecha"], "10 de octubre de 2026");
    assert_eq!(d["lugar_fecha"], "Madrid, 10 de octubre de 2026");
    assert_eq!(d["pem"], "7.005,15");
    assert_eq!(d["recargos"][0]["importe"], "910,67");
    assert_eq!(d["recargos"][1]["importe"], "420,31");
    assert_eq!(d["base"], "8.336,13");
    assert_eq!(d["iva"]["importe"], "1.750,59");
    assert_eq!(d["total"], "10.086,72");
    assert_eq!(
        d["total_letra"],
        "diez mil ochenta y seis euros con setenta y dos céntimos"
    );
    let c = &d["capitulos"][0];
    assert_eq!(
        (c["codigo"].as_str(), c["importe"].as_str()),
        (Some("C01"), Some("7.005,15"))
    );
    let p = &c["elementos"][0];
    assert_eq!(p["codigo"], "SR.M2");
    assert_eq!(
        (p["cantidad"].as_str(), p["precio"].as_str(), p["importe"].as_str()),
        (Some("210,00"), Some("29,89"), Some("6.276,90"))
    );
    // Medición: «A deducir hueco de escalera» −1 × 2,00 × 3,00 = −6,00
    let m = p["mediciones"].as_array().unwrap();
    assert_eq!(m.len(), 4);
    assert_eq!(m[3]["parcial"], "-6,00");
    assert_eq!(d["resumen"][0]["pct"], "100,00 %");
    assert_eq!(d["observaciones"].as_array().unwrap().len(), 6);
}

#[test]
fn pdf_completo_con_portada_logo_y_observaciones() {
    let bytes = pdf(&suelo_radiante(), &opciones()).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.len() > 10_000);
    if let Ok(dir) = std::env::var("PPTO_PDF_SALIDA") {
        std::fs::write(std::path::Path::new(&dir).join("ejemplo.pdf"), &bytes).unwrap();
    }
    // Sin GG/BI/IVA ni observaciones ni logo también compila
    let o = OpcionesPdf {
        iva: dec!(0),
        observaciones: vec![],
        ..OpcionesPdf::default()
    };
    assert!(pdf(&suelo_radiante(), &o).unwrap().starts_with(b"%PDF-"));
}

#[test]
fn errores_claros() {
    let o = OpcionesPdf {
        logo: Some("logo.bmp".into()),
        ..OpcionesPdf::default()
    };
    assert!(pdf(&suelo_radiante(), &o).unwrap_err().to_string().contains("PNG, JPG"));
    let dir = std::env::temp_dir().join(format!("ppto-pdf-{}.typ", std::process::id()));
    std::fs::write(&dir, "#let x = (\n").unwrap();
    let o = OpcionesPdf {
        plantilla: Some(dir.clone()),
        ..OpcionesPdf::default()
    };
    assert!(
        pdf(&suelo_radiante(), &o)
            .unwrap_err()
            .to_string()
            .contains("plantilla")
    );
    let _ = std::fs::remove_file(dir);
}
