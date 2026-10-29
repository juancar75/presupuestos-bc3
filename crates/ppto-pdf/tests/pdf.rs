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

#[test]
fn perfil_instaladora_sin_pem_gg_ni_bi() {
    // Instaladora: PEM 7.005,15 es la base; IVA 21 % = 1.471,08; total 8.476,23
    let mut o = opciones();
    ppto_pdf::Perfil::instaladora().aplicar(&mut o);
    let d = datos(&suelo_radiante(), &o, None).unwrap();
    assert_eq!(d["mostrar_pem"], false);
    assert!(d["recargos"].as_array().unwrap().is_empty());
    assert_eq!(d["etiqueta_base"], "Base imponible");
    assert_eq!(d["base"], "7.005,15");
    assert_eq!(d["iva"]["importe"], "1.471,08");
    assert_eq!(d["total"], "8.476,23");
    assert_eq!(d["tipo_documento"], "Oferta");
    assert!(d["observaciones"][1].as_str().unwrap().contains("puesta en marcha"));
    // Sin desglose y sin precios de venta, GG/BI no se aplican
    o.gastos_generales = dec!(13);
    o.precios_venta = false;
    assert_eq!(datos(&suelo_radiante(), &o, None).unwrap()["total"], "8.476,23");
    o.precios_venta = true;
    assert!(pdf(&suelo_radiante(), &o).unwrap().starts_with(b"%PDF-"));

    // Ingeniería: PEM + 13 % + 6 % + IVA = 10.086,72
    ppto_pdf::Perfil::ingenieria().aplicar(&mut o);
    let d = datos(&suelo_radiante(), &o, None).unwrap();
    assert_eq!(
        (d["mostrar_pem"].as_bool(), d["total"].as_str()),
        (Some(true), Some("10.086,72"))
    );
}

#[test]
fn perfiles_se_guardan_y_el_logo_se_copia() {
    let dir = std::env::temp_dir().join(format!("ppto-perfiles-{}", std::process::id()));
    let ruta = dir.join("perfiles.json");
    let mut p = ppto_pdf::Perfiles::cargar(&ruta);
    assert_eq!(p.lista.len(), 2, "de serie: instaladora e ingeniería");
    p.activo_mut().empresa = "Instalaciones Ejemplo, S.L.".into();
    p.duplicar();
    p.activo_mut().nombre = "Segunda empresa".into();
    let origen = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/datos/logo-sintetico.svg"));
    let copia = p.importar_logo(origen, &dir).unwrap();
    assert!(copia.starts_with(dir.join("logos")) && copia.exists());
    assert!(p.importar_logo(std::path::Path::new("logo.bmp"), &dir).is_err());
    p.guardar(&ruta).unwrap();
    let q = ppto_pdf::Perfiles::cargar(&ruta);
    assert_eq!(q, p);
    assert_eq!(q.activo().nombre, "Segunda empresa");
    assert_eq!(q.lista[0].empresa, "Instalaciones Ejemplo, S.L.");
    let mut r = q.clone();
    r.borrar_activo();
    r.borrar_activo();
    assert_eq!(r.lista.len(), 1, "siempre queda uno");
    let _ = std::fs::remove_dir_all(dir);
}

/// Instaladora con precios de venta: cada partida sale a PEM + GG + BI (lo
/// mismo que su desglose en el programa) y capítulos y total suman esos
/// importes; ni PEM ni GG ni BI aparecen en el resumen.
#[test]
fn instaladora_con_precios_de_venta_por_partida() {
    use ppto_core::formato::eur;
    use ppto_core::{Naturaleza, redondear, venta};
    let p = suelo_radiante();
    let mut o = opciones();
    ppto_pdf::Perfil::instaladora().aplicar(&mut o);
    assert!(o.precios_venta);
    o.gastos_generales = dec!(13);
    o.beneficio_industrial = dec!(6);
    let d = datos(&p, &o, None).unwrap();
    assert!(d["recargos"].as_array().unwrap().is_empty());
    assert_eq!(d["mostrar_pem"], false);

    // Partida a partida, como el programa
    let v = p.valorar().unwrap();
    let mut esperado = ppto_core::Decimal::ZERO;
    let mut vistas = 0;
    fn partidas(x: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
        for e in x["elementos"].as_array().into_iter().flatten() {
            if e["tipo"] == "partida" {
                out.push(e.clone());
            } else {
                partidas(e, out);
            }
        }
    }
    let mut todas = Vec::new();
    for c in d["capitulos"].as_array().unwrap() {
        partidas(c, &mut todas);
    }
    for (cap, ls) in &v.lineas {
        if p.conceptos[cap].naturaleza != Naturaleza::Capitulo {
            continue;
        }
        for l in ls {
            if p.conceptos[&l.hijo].naturaleza == Naturaleza::Capitulo {
                continue;
            }
            let pv = venta::venta_partida(l.precio, dec!(13), dec!(6), 2).venta;
            let imp = redondear(pv * l.cantidad, 2);
            esperado += imp;
            let j = todas.iter().find(|j| j["codigo"] == l.hijo.as_str()).unwrap();
            assert_eq!(j["precio"], eur(pv).as_str());
            assert_eq!(j["importe"], eur(imp).as_str());
            vistas += 1;
        }
    }
    assert!(vistas > 0);
    assert_eq!(d["base"], eur(esperado).as_str());
    assert!(esperado > v.pem);
}

/// Una partida con margen propio sale en el PDF a PEM + su margen; las
/// demás, con GG + BI.
#[test]
fn margen_propio_de_partida_en_el_pdf() {
    use ppto_core::formato::eur;
    use ppto_core::{redondear, venta};
    let mut p = suelo_radiante();
    p.fijar_margen("SR.M2", Some(dec!(30))).unwrap();
    let mut o = opciones();
    ppto_pdf::Perfil::instaladora().aplicar(&mut o);
    o.gastos_generales = dec!(13);
    o.beneficio_industrial = dec!(6);
    let d = datos(&p, &o, None).unwrap();
    let texto = d.to_string();
    let v = p.valorar().unwrap();
    let l = v.lineas.values().flatten().find(|l| l.hijo == "SR.M2").unwrap();
    let pv = venta::venta_con_margen(l.precio, Some(dec!(30)), dec!(13), dec!(6), 2).venta;
    assert_eq!(pv, l.precio + redondear(l.precio * dec!(0.3), 2));
    assert!(texto.contains(&format!("\"precio\":\"{}\"", eur(pv))), "{texto}");
}

#[test]
fn pedido_a_fabricante_y_a_subcontrata_en_pdf() {
    use ppto_core::pedidos::pedidos;
    let mut p = ppto_core::ejemplos::suelo_radiante();
    p.fijar_descuento("SCHAKO", rust_decimal_macros::dec!(40)).unwrap();
    p.fijar_tarifa("MT.PANEL", "schako", rust_decimal_macros::dec!(20))
        .unwrap();
    let o = ppto_pdf::DatosPedido {
        empresa: "Instaladora de ejemplo, S.L.".into(),
        fecha: "29/10/2026".into(),
        proveedor: vec![("CIF".into(), "B00000000".into())],
        condiciones: "Portes pagados. Entrega en obra.".into(),
        ..Default::default()
    };
    for x in pedidos(&p).unwrap() {
        let b = ppto_pdf::pdf_pedido(&p, &x, &o).unwrap();
        assert!(b.starts_with(b"%PDF"), "{}", x.nombre());
    }
}
