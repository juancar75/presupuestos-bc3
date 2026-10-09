// SPDX-License-Identifier: GPL-3.0-or-later
//! Pruebas del importador BC3 con ficheros sintéticos (tests/datos).

use ppto_bc3::{Gravedad, importar, importar_fichero};
use ppto_core::concepto::Naturaleza;
use ppto_core::ejemplos::suelo_radiante;
use rust_decimal_macros::dec;

fn datos(nombre: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/datos")
        .join(nombre)
}

#[test]
fn suelo_radiante_coincide_con_el_ejemplo_calculado_a_mano() {
    let imp = importar_fichero(datos("suelo-radiante.bc3")).unwrap();
    let p = &imp.presupuesto;

    // Cabecera, ~K y conteo de registros
    assert_eq!(imp.cabecera.juego_caracteres, "ANSI");
    assert_eq!(imp.cabecera.version_formato, "FIEBDC-3/2020");
    assert_eq!(imp.porcentajes.gastos_generales, Some(dec!(13)));
    assert_eq!(imp.porcentajes.beneficio_industrial, Some(dec!(6)));
    assert_eq!(imp.porcentajes.iva, Some(dec!(21)));
    assert_eq!(imp.registros["C"], 11);
    assert_eq!(imp.registros["M"], 2);

    // Sin errores ni avisos: solo información (criterio de % y ~L no importado)
    assert_eq!(imp.errores(), 0, "{:#?}", imp.incidencias);
    assert_eq!(imp.avisos(), 0, "{:#?}", imp.incidencias);

    // Mismos precios que el ejemplo calculado a mano y sin discrepancias con lo declarado
    let esperado = suelo_radiante().valorar().unwrap();
    let v = p.valorar().unwrap();
    for (codigo, precio) in &esperado.precios {
        assert_eq!(v.precios.get(codigo), Some(precio), "{codigo}");
    }
    assert_eq!(v.pem, dec!(7005.15));
    assert_eq!(imp.pem_declarado, Some(dec!(7005.15)));
    assert!(imp.discrepancias.is_empty(), "{:?}", imp.discrepancias);

    // Naturalezas, raíz, textos y mediciones
    assert_eq!(p.raiz, "OBRA");
    assert_eq!(p.concepto("C01").unwrap().naturaleza, Naturaleza::Capitulo);
    assert_eq!(p.concepto("%MA").unwrap().naturaleza, Naturaleza::Porcentaje);
    assert_eq!(p.concepto("MO.OF1F").unwrap().naturaleza, Naturaleza::ManoObra);
    assert_eq!(p.concepto("MT.TUBO16").unwrap().naturaleza, Naturaleza::Material);
    assert!(
        p.concepto("SR.M2")
            .unwrap()
            .texto
            .as_deref()
            .unwrap()
            .contains("estanqueidad")
    );
    let m = &p.mediciones[&("C01".into(), "SR.M2".into())];
    assert_eq!(m.lineas.len(), 4);
    assert_eq!(m.lineas[2].comentario, "Baños");
    assert_eq!(m.lineas[3].unidades, Some(dec!(-1)));
    assert_eq!(m.lineas[0].altura, None);

    // Acentos y símbolos de cp1252
    assert_eq!(
        p.concepto("MO.OF1F").unwrap().resumen,
        "Oficial 1ª instalador de calefacción"
    );
    assert!(p.concepto("SR.M2").unwrap().resumen.ends_with("16×2"));

    // Recursos idénticos a los del ejemplo
    let e = p.explosion_recursos().unwrap();
    assert_eq!(e.recurso("MT.TUBO16").unwrap().cantidad, dec!(1470));
    assert_eq!(e.descuadre_redondeo, dec!(2.10));
}

#[test]
fn fichero_con_errores_se_importa_y_los_explica() {
    let imp = importar_fichero(datos("con-errores.bc3")).unwrap();
    let hay = |g: Gravedad, texto: &str| {
        imp.incidencias
            .iter()
            .any(|i| i.gravedad == g && i.mensaje.contains(texto))
    };
    let todas: Vec<String> = imp.incidencias.iter().map(ToString::to_string).collect();
    assert!(hay(Gravedad::Aviso, "texto antes del primer registro"), "{todas:#?}");
    assert!(hay(Gravedad::Error, "«PFANTASMA» se usa"), "{todas:#?}");
    assert!(hay(Gravedad::Error, "factor no numérico «x»"), "{todas:#?}");
    assert!(hay(Gravedad::Error, "precio no numérico «abc»"), "{todas:#?}");
    assert!(hay(Gravedad::Aviso, "R1: concepto repetido"), "{todas:#?}");
    assert!(hay(Gravedad::Error, "referencia circular P2 → P3 → P2"), "{todas:#?}");
    assert!(hay(Gravedad::Error, "NOEXISTE"), "{todas:#?}");
    assert!(hay(Gravedad::Aviso, "~Z"), "{todas:#?}");
    assert!(hay(Gravedad::Aviso, "texto de «NADIE»"), "{todas:#?}");
    assert!(hay(Gravedad::Aviso, "sin concepto raíz"), "{todas:#?}");
    assert!(
        hay(Gravedad::Aviso, "suman 3,00 pero la cantidad es 2,00"),
        "{todas:#?}"
    );

    // Las incidencias llevan la línea del fichero (la primera línea es texto suelto)
    let fantasma = imp
        .incidencias
        .iter()
        .find(|i| i.mensaje.contains("PFANTASMA"))
        .unwrap();
    assert_eq!(fantasma.linea, 4);

    // El presupuesto se puede valorar: P1 = 2,5 × 10 = 25,00 (declarado 45)
    let p = &imp.presupuesto;
    assert_eq!(p.precio("P1").unwrap(), dec!(25.00));
    let d = imp.discrepancias.iter().find(|d| d.codigo == "P1").unwrap();
    assert_eq!((d.declarado, d.calculado), (dec!(45.00), dec!(25.00)));
    // CAP1 = 2 × 25 + P2 (10, tras romper el ciclo) + PFANTASMA (0) = 60,00 (declarado 100)
    let d = imp.discrepancias.iter().find(|d| d.codigo == "CAP1").unwrap();
    assert_eq!((d.declarado, d.calculado), (dec!(100.00), dec!(60.00)));
    // Raíz sintética con los dos capítulos sueltos
    assert_eq!(p.concepto(&p.raiz).unwrap().descomposicion.len(), 2);
    // Ordenadas por gravedad: primero los errores
    assert_eq!(imp.incidencias[0].gravedad, Gravedad::Error);
}

#[test]
fn utf8_declarado() {
    let imp = importar_fichero(datos("utf8.bc3")).unwrap();
    assert_eq!(imp.errores(), 0);
    let a = imp.presupuesto.concepto("A").unwrap();
    assert_eq!(a.resumen, "Año, señal, pingüino — 12,5 €");
    assert_eq!(imp.presupuesto.pem().unwrap(), dec!(12.50));
}

#[test]
fn no_es_un_bc3() {
    assert!(importar(b"").is_err());
    assert!(importar(b"hola, esto no es un presupuesto").is_err());
    assert!(importar(b"~V|x|y|z|").is_err());
}

#[test]
fn ficheros_truncados_o_corruptos_no_bloquean_el_programa() {
    let bytes = std::fs::read(datos("suelo-radiante.bc3")).unwrap();
    for n in 0..bytes.len() {
        let _ = importar(&bytes[..n]);
    }
    // Bytes alterados de forma determinista
    let mut corrupto = bytes.clone();
    for (i, b) in corrupto.iter_mut().enumerate() {
        if i % 7 == 3 {
            *b = b.wrapping_mul(31).wrapping_add(i as u8);
        }
    }
    let _ = importar(&corrupto);
    // Sin separadores de fin de registro y con «~» sueltos
    let _ = importar(b"~~~~|||\\\\\\~C|~D|X|X\\1\\1\\|~C|X||x|1|");
}

#[test]
fn porcentaje_sin_precio_toma_el_rendimiento_como_puntos() {
    let bc3 = "~V|x|FIEBDC-3/2020|x||ANSI|\n\
               ~C|R##||r|0|\n~D|R##|P\\1\\1\\|\n\
               ~C|P|ud|p|0|\n~D|P|M\\1\\1\\%X\\1\\3\\|\n\
               ~C|M|ud|m|100||3|\n~C|%X|%|tres por ciento|0|\n";
    let imp = importar(bc3.as_bytes()).unwrap();
    // 100 + 3 % = 103,00
    assert_eq!(imp.presupuesto.precio("P").unwrap(), dec!(103.00));
}

#[test]
fn bc3_grande_de_cuatro_mil_partidas() {
    use std::fmt::Write;
    let mut s = String::from("~V|x|FIEBDC-3/2020|x|Grande|ANSI|\r\n~C|OBRA##||Obra grande|0|\r\n~D|OBRA##|");
    for c in 0..40 {
        write!(s, "C{c:02}\\1\\1\\").unwrap();
    }
    s.push_str("|\r\n");
    for r in 0..500 {
        writeln!(
            s,
            "~C|R{r:03}|ud|Recurso {r}|{}.{:02}||{}|\r",
            1 + r % 90,
            r % 100,
            1 + r % 3
        )
        .unwrap();
    }
    for c in 0..40 {
        write!(s, "~C|C{c:02}#||Capítulo {c}|0|\r\n~D|C{c:02}#|").unwrap();
        for i in 0..100 {
            write!(s, "P{c:02}{i:03}\\1\\{}\\", 1 + i % 20).unwrap();
        }
        s.push_str("|\r\n");
        for i in 0..100 {
            write!(s, "~C|P{c:02}{i:03}|m2|Partida {c}-{i}|0|\r\n~D|P{c:02}{i:03}|").unwrap();
            for k in 0..6 {
                write!(s, "R{:03}\\1\\0.{}\\", (c * 100 + i + k * 37) % 500, 1 + k).unwrap();
            }
            s.push_str("|\r\n");
        }
    }
    let t = std::time::Instant::now();
    let imp = importar(s.as_bytes()).unwrap();
    let ms = t.elapsed().as_millis();
    assert_eq!(imp.presupuesto.conceptos.len(), 1 + 500 + 40 + 4000);
    assert_eq!(imp.errores(), 0);
    // Los precios declarados son 0: cada partida aparece como discrepancia (es lo esperado)
    assert_eq!(imp.discrepancias.len(), 4000 + 40 + 1);
    eprintln!("BC3 de {} KB importado y recalculado en {ms} ms", s.len() / 1024);
}
