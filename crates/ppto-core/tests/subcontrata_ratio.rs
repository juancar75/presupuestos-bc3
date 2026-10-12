// SPDX-License-Identifier: GPL-3.0-or-later
//! Subcontrata a ratio aplicada al presupuesto, con cifras calculadas a mano.

use ppto_core::ejemplos::suelo_radiante;
use ppto_core::ratio::PaqueteRatio;
use ppto_core::{Naturaleza as N, Presupuesto};
use rust_decimal_macros::dec;
use std::collections::BTreeMap;

/// Obra VRF sintética:
/// - P.INT (u)  = 1 máquina interior 900 + 2 h oficial 25 + 2 h ayudante 20 + 3 % MA = 1.019,70 → 24 ud
/// - P.EXT (u)  = 1 máquina exterior 4.000 + 4 h + 4 h + 3 % MA                     = 4.305,40 →  3 ud
/// - P.FRIG (m) = 1 m tubería 6 + 0,2 h oficial + 3 % MA                             =    11,33 → 300 m
/// - P.CAB (m)  = 1 m cable 1,50 + 0,05 h oficial (sin MA)                           =     2,75 → 400 m
///
/// Coste directo: 24.472,80 + 12.916,20 + 3.399,00 + 1.100,00 = 41.888,00
fn obra_vrf() -> Presupuesto {
    let mut p = Presupuesto::new("VRF", "OBRA", "Obra VRF");
    p.nuevo_capitulo("OBRA", "C01", "Climatización").unwrap();
    for (c, u, n, pr) in [
        ("MO.OF", "h", N::ManoObra, dec!(25)),
        ("MO.AY", "h", N::ManoObra, dec!(20)),
        ("MAQ.INT", "u", N::Material, dec!(900)),
        ("MAQ.EXT", "u", N::Material, dec!(4000)),
        ("TUB", "m", N::Material, dec!(6)),
        ("CAB", "m", N::Material, dec!(1.5)),
    ] {
        p.nuevo_recurso(c, u, c, n, pr).unwrap();
    }
    p.nuevo_recurso("%MA", "%", "Medios auxiliares", N::Porcentaje, dec!(0))
        .unwrap();
    type Lineas<'a> = &'a [(&'a str, rust_decimal::Decimal)];
    let partidas: [(&str, &str, Lineas, rust_decimal::Decimal); 4] = [
        (
            "P.INT",
            "u",
            &[
                ("MAQ.INT", dec!(1)),
                ("MO.OF", dec!(2)),
                ("MO.AY", dec!(2)),
                ("%MA", dec!(3)),
            ],
            dec!(24),
        ),
        (
            "P.EXT",
            "u",
            &[
                ("MAQ.EXT", dec!(1)),
                ("MO.OF", dec!(4)),
                ("MO.AY", dec!(4)),
                ("%MA", dec!(3)),
            ],
            dec!(3),
        ),
        (
            "P.FRIG",
            "m",
            &[("TUB", dec!(1)), ("MO.OF", dec!(0.2)), ("%MA", dec!(3))],
            dec!(300),
        ),
        ("P.CAB", "m", &[("CAB", dec!(1)), ("MO.OF", dec!(0.05))], dec!(400)),
    ];
    for (c, u, lineas, q) in partidas {
        p.nueva_partida("C01", c, u, c).unwrap();
        for (h, r) in lineas {
            p.anadir_linea(c, h, *r).unwrap();
        }
        p.fijar_rendimiento("C01", c, q).unwrap();
    }
    p
}

fn paquete_vrf() -> PaqueteRatio {
    let mo = |extra: &[&str]| {
        let mut v = vec!["MO.OF".to_owned()];
        v.extend(extra.iter().map(|s| (*s).to_owned()));
        v
    };
    PaqueteRatio {
        codigo: "SUB.VRF".into(),
        descripcion: "Montaje VRF por máquina".into(),
        unidad: "máquina".into(),
        precio: dec!(650),
        medidor: vec!["P.INT".into(), "P.EXT".into()],
        asignaciones: BTreeMap::from([
            ("P.INT".to_owned(), mo(&["MO.AY"])),
            ("P.EXT".to_owned(), mo(&["MO.AY"])),
            ("P.FRIG".to_owned(), mo(&["TUB"])),
            ("P.CAB".to_owned(), mo(&["CAB"])),
        ]),
    }
}

#[test]
fn vrf_a_ratio_por_maquina() {
    let mut p = obra_vrf();
    assert_eq!(p.coste_directo().unwrap(), dec!(41888.00));
    let inf = p.aplicar_ratio(&paquete_vrf()).unwrap();
    // Medidor: 24 interiores + 3 exteriores = 27 máquinas × 650 = 17.550,00
    assert_eq!((inf.medidor, inf.importe), (dec!(27), dec!(17550.00)));
    // Retirado: 90 × 24 + 180 × 3 + 11 × 300 + 2,75 × 400 = 2.160 + 540 + 3.300 + 1.100 = 7.100
    assert_eq!(inf.retirado, dec!(7100.00));
    assert_eq!(inf.coste_antes, dec!(41888.00));
    // Horas: oficial 2×24 + 4×3 + 0,2×300 + 0,05×400 = 140; ayudante 2×24 + 4×3 = 60
    assert_eq!(inf.horas_liberadas["MO.OF"], dec!(140));
    assert_eq!(inf.horas_liberadas["MO.AY"], dec!(60));
    // Reparto en proporción al coste retirado; la suma de máquinas es exactamente 27
    let suma: rust_decimal::Decimal = inf.reparto.iter().map(|r| r.rendimiento * r.cantidad).sum();
    assert_eq!(suma, dec!(27));
    let k: BTreeMap<_, _> = inf
        .reparto
        .iter()
        .map(|r| (r.partida.as_str(), r.rendimiento))
        .collect();
    assert_eq!(k["P.INT"], dec!(0.342254)); // 27 × 2.160/7.100 / 24
    assert_eq!(k["P.FRIG"], dec!(0.041831)); // 27 × 3.300/7.100 / 300
    assert_eq!(k["P.CAB"], dec!(0.010458)); // 27 × 1.100/7.100 / 400
    assert_eq!(k["P.EXT"], dec!(0.684468), "absorbe el redondeo (menor cantidad)");
    // Descompuestos: se quedan las máquinas y los medios auxiliares
    let hijos = |c: &str| {
        p.concepto(c)
            .unwrap()
            .descomposicion
            .iter()
            .map(|l| l.hijo.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(hijos("P.INT"), ["MAQ.INT", "SUB.VRF", "%MA"]);
    assert_eq!(hijos("P.FRIG"), ["SUB.VRF", "%MA"]);
    assert_eq!(p.concepto("SUB.VRF").unwrap().naturaleza, N::Subcontrata);
    // P.INT = 900 + 0,342254 × 650 (222,47) = 1.122,47; MA 3 %: 11,225 × 3 = 33,68 → 1.156,15
    assert_eq!(p.precio("P.INT").unwrap(), dec!(1156.15));
    assert_eq!(p.precio("P.EXT").unwrap(), dec!(4578.25));
    assert_eq!(p.precio("P.FRIG").unwrap(), dec!(28.01));
    assert_eq!(p.precio("P.CAB").unwrap(), dec!(6.80));
    assert_eq!(inf.coste_despues, dec!(52605.35));
    assert_eq!(inf.ahorro(), dec!(-10717.35));
}

#[test]
fn partida_entera_o_solo_mano_de_obra() {
    // Caso 1: SR.M2 entera (todo menos medios auxiliares) a 38 €/m²
    // → 38,00 + 2 % MA (0,38 × 2 = 0,76) = 38,76 €/m²
    let mut p = suelo_radiante();
    let todos: Vec<String> = p
        .concepto("SR.M2")
        .unwrap()
        .descomposicion
        .iter()
        .map(|l| l.hijo.clone())
        .filter(|h| h != "%MA")
        .collect();
    p.aplicar_ratio(&PaqueteRatio {
        codigo: "SUB.SR".into(),
        descripcion: "Suelo radiante completo".into(),
        unidad: "m2".into(),
        precio: dec!(38),
        medidor: vec!["SR.M2".into()],
        asignaciones: BTreeMap::from([("SR.M2".into(), todos)]),
    })
    .unwrap();
    assert_eq!(p.precio("SR.M2").unwrap(), dec!(38.76));
    assert_eq!(p.concepto("SR.M2").unwrap().descomposicion[0].rendimiento, dec!(1));

    // Caso 2: solo la mano de obra de SR.M2 a 9 €/m²
    // Material 9,98 + 7,70 + 0,24 = 17,92; + 9,00 = 26,92; MA 2 %: 0,269 × 2 = 0,538 → 0,54 → 27,46
    let mut p = suelo_radiante();
    p.aplicar_ratio(&PaqueteRatio {
        codigo: "SUB.MO".into(),
        descripcion: "Montaje suelo radiante".into(),
        unidad: "m2".into(),
        precio: dec!(9),
        medidor: vec!["SR.M2".into()],
        asignaciones: BTreeMap::from([("SR.M2".into(), vec!["MO.OF1F".into(), "MO.AYUF".into()])]),
    })
    .unwrap();
    assert_eq!(p.precio("SR.M2").unwrap(), dec!(27.46));
}

#[test]
fn errores_sin_tocar_el_presupuesto() {
    let mut p = obra_vrf();
    let antes = p.clone();
    let mut paq = paquete_vrf();
    paq.asignaciones.insert("P.INT".into(), vec!["%MA".into()]);
    assert!(
        p.aplicar_ratio(&paq)
            .unwrap_err()
            .to_string()
            .contains("medios auxiliares")
    );
    let mut paq = paquete_vrf();
    paq.precio = dec!(0);
    assert!(p.aplicar_ratio(&paq).is_err());
    let mut paq = paquete_vrf();
    paq.medidor.clear();
    assert!(p.aplicar_ratio(&paq).unwrap_err().to_string().contains("medidor"));
    let mut paq = paquete_vrf();
    paq.asignaciones.insert("P.CAB".into(), vec!["MAQ.INT".into()]);
    assert!(p.aplicar_ratio(&paq).is_err(), "recurso que no está en la partida");
    assert_eq!(p, antes);
    // Dos veces el mismo código: la segunda se rechaza
    p.aplicar_ratio(&paquete_vrf()).unwrap();
    let despues = p.clone();
    assert!(
        p.aplicar_ratio(&paquete_vrf())
            .unwrap_err()
            .to_string()
            .contains("ya existe")
    );
    assert_eq!(p, despues);
}
