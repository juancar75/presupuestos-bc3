// SPDX-License-Identifier: GPL-3.0-or-later
//! Propuestas de la IA: se aplican solo al aceptarlas (cifras a mano).

use ppto_core::Naturaleza as N;
use ppto_core::ejemplos::suelo_radiante;
use ppto_core::propuesta::{Confianza, Contenido, EstadoPropuesta, LineaPropuesta, Propuesta};
use rust_decimal_macros::dec;

fn linea(codigo: &str, r: rust_decimal::Decimal) -> LineaPropuesta {
    LineaPropuesta {
        codigo: codigo.into(),
        rendimiento: r,
        naturaleza: None,
        unidad: String::new(),
        resumen: String::new(),
        precio: None,
        pvp: None,
        fabricante: None,
        grupo: None,
    }
}

fn propuesta(partida: &str, contenido: Contenido) -> Propuesta {
    Propuesta {
        id: 1,
        partida: partida.into(),
        contenido,
        confianza: Confianza::Media,
        justificacion: "prueba".into(),
        autor: "IA".into(),
        estado: EstadoPropuesta::Pendiente,
        descuentos: Default::default(),
    }
}

#[test]
fn descompuesto_con_recursos_existentes_y_nuevos() {
    let mut p = suelo_radiante();
    p.nueva_partida("C01", "P.VAL", "ud", "Válvula de zona").unwrap();
    let prop = propuesta(
        "P.VAL",
        Contenido::Descompuesto(vec![
            LineaPropuesta {
                precio: Some(dec!(26)), // en la obra está a 24,50: se usa el de la obra
                ..linea("MO.OF1F", dec!(0.5))
            },
            LineaPropuesta {
                naturaleza: Some(N::Material),
                unidad: "ud".into(),
                resumen: "Válvula de zona 3/4\"".into(),
                precio: Some(dec!(30)),
                ..linea("MT.VAL", dec!(1))
            },
            linea("%MA", dec!(2)),
        ]),
    );
    // Vista previa sin cambiar nada
    let antes = p.clone();
    let sim = p.simular_propuesta(&prop).unwrap();
    assert_eq!(p, antes);
    // 0,5 × 24,50 = 12,25 + 30 = 42,25; MA: 0,4225 → 0,423 × 2 = 0,846 → 0,85 → 43,10
    assert_eq!(sim.precio_despues, dec!(43.10));
    let a = p.aplicar_propuesta(&prop).unwrap();
    assert_eq!((a.precio_antes, a.precio_despues), (dec!(0), dec!(43.10)));
    assert_eq!(a.nuevos, ["MT.VAL"]);
    assert_eq!(a.precios_distintos, [("MO.OF1F".to_owned(), dec!(24.50), dec!(26))]);
    assert_eq!(p.concepto("MT.VAL").unwrap().naturaleza, N::Material);
    assert_eq!(
        p.concepto("MO.OF1F").unwrap().precio,
        dec!(24.50),
        "no cambia el recurso de la obra"
    );
}

#[test]
fn precio_directo_y_errores() {
    let mut p = suelo_radiante();
    p.nueva_partida("C01", "P.DIR", "ud", "Partida").unwrap();
    let a = p
        .aplicar_propuesta(&propuesta("P.DIR", Contenido::PrecioDirecto(dec!(57.30))))
        .unwrap();
    assert_eq!(a.precio_despues, dec!(57.30));

    let antes = p.clone();
    let mal = [
        propuesta("SR.M2", Contenido::PrecioDirecto(dec!(10))), // tiene descompuesto
        propuesta("C01", Contenido::PrecioDirecto(dec!(10))),   // no es partida
        propuesta("P.DIR", Contenido::Descompuesto(vec![])),
        propuesta("P.DIR", Contenido::Descompuesto(vec![linea("NO.EXISTE", dec!(1))])),
        propuesta("P.DIR", Contenido::Descompuesto(vec![linea("MO.OF1F", dec!(0))])),
        propuesta("P.DIR", Contenido::Descompuesto(vec![linea("C01", dec!(1))])),
        propuesta(
            "P.DIR",
            Contenido::Descompuesto(vec![
                linea("MO.OF1F", dec!(1)),
                LineaPropuesta {
                    naturaleza: Some(N::Material),
                    unidad: "ud".into(),
                    resumen: "x".into(),
                    precio: None, // nuevo sin precio
                    ..linea("MT.SIN", dec!(1))
                },
            ]),
        ),
    ];
    for m in &mal {
        assert!(p.aplicar_propuesta(m).is_err(), "{m:?}");
        assert_eq!(p, antes, "un error no deja nada a medias");
    }
}

#[test]
fn material_de_tarifa_con_descuento_del_fabricante() {
    let mut p = suelo_radiante();
    p.nueva_partida("C01", "P.CF", "ud", "Compuerta cortafuegos").unwrap();
    p.nueva_partida("C01", "P.CF2", "ud", "Otra compuerta").unwrap();
    let cf = |partida: &str, codigo: &str, pvp| {
        let mut x = propuesta(
            partida,
            Contenido::Descompuesto(vec![
                LineaPropuesta {
                    naturaleza: Some(N::Material),
                    unidad: "ud".into(),
                    resumen: "Compuerta cortafuegos".into(),
                    pvp: Some(pvp),
                    fabricante: Some("koolair".into()),
                    ..linea(codigo, dec!(1))
                },
                linea("MO.OF1F", dec!(1)),
            ]),
        );
        x.descuentos.insert("KOOLAIR".into(), dec!(40)); // estimación de la IA
        x
    };
    // La obra no tiene descuento KOOLAIR: se toma el estimado (40 %)
    let a = p.aplicar_propuesta(&cf("P.CF", "MT.CF", dec!(500))).unwrap();
    assert_eq!(a.descuentos_nuevos, [("KOOLAIR".to_owned(), dec!(40))]);
    // 500 × 0,60 = 300 + 24,50 = 324,50
    assert_eq!(p.precio("P.CF").unwrap(), dec!(324.50));
    // El usuario fija su descuento real (45 %): la siguiente propuesta lo respeta
    let cambiados = p.fijar_descuento("Koolair", dec!(45)).unwrap();
    assert_eq!(cambiados, ["MT.CF"]);
    assert_eq!(p.precio("P.CF").unwrap(), dec!(299.50), "500 × 0,55 = 275 + 24,50");
    let a = p.aplicar_propuesta(&cf("P.CF2", "MT.CF2", dec!(800))).unwrap();
    assert!(a.descuentos_nuevos.is_empty(), "manda el descuento de la obra");
    assert_eq!(p.concepto("MT.CF2").unwrap().precio, dec!(440), "800 × 0,55");
    // Escribir el neto a mano rompe el vínculo con la tarifa
    p.fijar_precio("MT.CF2", dec!(350)).unwrap();
    assert!(p.concepto("MT.CF2").unwrap().tarifa.is_none());
    assert_eq!(p.fijar_descuento("KOOLAIR", dec!(50)).unwrap(), ["MT.CF"]);
    assert_eq!(p.concepto("MT.CF2").unwrap().precio, dec!(350));
    assert_eq!(p.concepto("MT.CF").unwrap().precio, dec!(250));
    // Fabricantes
    let f = p.fabricantes();
    assert_eq!(
        (f[0].nombre.as_str(), f[0].descuento, f[0].recursos),
        ("KOOLAIR", dec!(50), 1)
    );
    // Errores
    assert!(p.fijar_descuento("KOOLAIR", dec!(100)).is_err());
    assert!(p.fijar_descuento("", dec!(10)).is_err());
    let mut sin_fab = cf("P.CF2", "MT.CF3", dec!(10));
    if let Contenido::Descompuesto(l) = &mut sin_fab.contenido {
        l[0].fabricante = None;
    }
    assert!(p.aplicar_propuesta(&sin_fab).is_err(), "PVP sin fabricante");
}

#[test]
fn recurso_a_neto_se_liga_a_la_tarifa() {
    let mut p = suelo_radiante();
    p.fijar_descuento("SCHAKO", dec!(40)).unwrap();
    // MT.PANEL está en la obra a 9,50 € neto, sin tarifa
    let mut prop = propuesta(
        "SR.M2",
        Contenido::Descompuesto(vec![LineaPropuesta {
            pvp: Some(dec!(20)),
            fabricante: Some("Schako".into()),
            ..linea("MT.PANEL", dec!(1))
        }]),
    );
    prop.descuentos.insert("SCHAKO".into(), dec!(30)); // la obra ya tiene 40 %: manda la obra
    let a = p.aplicar_propuesta(&prop).unwrap();
    assert_eq!(a.ligados, [("MT.PANEL".to_owned(), dec!(9.50), dec!(12))], "20 × 0,60");
    assert!(a.descuentos_nuevos.is_empty());
    assert_eq!(p.precio("SR.M2").unwrap(), dec!(12));
    p.fijar_descuento("SCHAKO", dec!(50)).unwrap();
    assert_eq!(p.precio("SR.M2").unwrap(), dec!(10));
}

#[test]
fn descuento_por_grupo_de_producto() {
    let mut p = suelo_radiante();
    p.fijar_descuento("S&P", dec!(40)).unwrap();
    p.fijar_tarifa_grupo("MT.PANEL", "s&p", Some("e08"), dec!(10)).unwrap();
    p.fijar_tarifa_grupo("MT.TUBO16", "S&P", Some("E22"), dec!(2)).unwrap();
    // Sin descuento de grupo: el general (40 %)
    assert_eq!(p.concepto("MT.PANEL").unwrap().precio, dec!(6));
    // Grupo E08 con descuento propio: solo cambia MT.PANEL
    assert_eq!(p.fijar_descuento("S&P/E08", dec!(52)).unwrap(), ["MT.PANEL"]);
    assert_eq!(p.concepto("MT.PANEL").unwrap().precio, dec!(4.80));
    assert_eq!(
        p.concepto("MT.TUBO16").unwrap().precio,
        dec!(1.20),
        "E22 sigue con el general"
    );
    // El general solo afecta a los que no tienen grupo con descuento propio
    assert_eq!(p.fijar_descuento("S&P", dec!(45)).unwrap(), ["MT.TUBO16"]);
    assert_eq!(p.concepto("MT.PANEL").unwrap().precio, dec!(4.80));
    assert_eq!(p.concepto("MT.TUBO16").unwrap().precio, dec!(1.10));
    // Tabla
    let f = p.fabricantes();
    let filas: Vec<_> = f.iter().map(|x| (x.nombre.as_str(), x.descuento, x.recursos)).collect();
    assert_eq!(filas, [("S&P", dec!(45), 1), ("S&P/E08", dec!(52), 1)]);
    // Quitar el del grupo: vuelve al general
    p.quitar_descuento("S&P/E08");
    assert_eq!(p.concepto("MT.PANEL").unwrap().precio, dec!(5.50));
    // Cambiar el grupo de un recurso
    p.fijar_descuento("S&P/E22", dec!(60)).unwrap();
    p.fijar_grupo("MT.PANEL", "E22").unwrap();
    assert_eq!(p.concepto("MT.PANEL").unwrap().precio, dec!(4));
    p.fijar_grupo("MT.PANEL", "").unwrap();
    assert_eq!(p.concepto("MT.PANEL").unwrap().tarifa.as_ref().unwrap().grupo, None);
    assert_eq!(p.concepto("MT.PANEL").unwrap().precio, dec!(5.50));
}

/// Obra ya importada con las unidades de obra como «otros» (ciego de Presto
/// leído con una versión anterior): la propuesta la convierte en partida.
#[test]
fn propuesta_sobre_unidad_de_obra_otros_en_capitulo() {
    let mut p = suelo_radiante();
    p.nueva_partida("C01", "U.TUBO", "m", "Tubo").unwrap();
    p.conceptos.get_mut("U.TUBO").unwrap().naturaleza = N::Otros;
    let prop = propuesta(
        "U.TUBO",
        Contenido::Descompuesto(vec![LineaPropuesta {
            naturaleza: Some(N::Material),
            unidad: "m".into(),
            resumen: "Tubo".into(),
            precio: Some(dec!(4)),
            ..linea("MT.TUBO", dec!(1))
        }]),
    );
    p.aplicar_propuesta(&prop).unwrap();
    assert_eq!(p.conceptos["U.TUBO"].naturaleza, N::Partida);
    assert_eq!(p.precio("U.TUBO").unwrap(), dec!(4));
    // Un recurso básico que no cuelga de un capítulo sigue sin admitirse
    let mal = propuesta("MO.OF1F", Contenido::Descompuesto(vec![linea("MT.TUBO", dec!(1))]));
    assert!(p.aplicar_propuesta(&mal).is_err());
}
