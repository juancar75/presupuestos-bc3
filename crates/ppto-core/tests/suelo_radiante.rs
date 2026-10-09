// SPDX-License-Identifier: GPL-3.0-or-later
//! Caso de validación «suelo radiante» (P-003, P-006, P-007, P-008, P-017).
//! Cada valor esperado está calculado a mano en
//! docs/casos-validacion/suelo-radiante.md. Si cambia un valor aquí, debe
//! cambiar allí y pasar revisión técnica (etiqueta needs-engineering-review).

use ppto_core::concepto::Naturaleza;
use ppto_core::ejemplos::{ofertas_montaje_suelo_radiante, suelo_radiante};
use ppto_core::subcontrata::{Asignacion, Contratacion, PaqueteTrabajo, simular};
use ppto_core::{Decimal, ErrorMotor, venta};
use rust_decimal_macros::dec;

#[test]
fn descompuesto_suelo_radiante() {
    let p = suelo_radiante();
    let l = p.lineas_valoradas("SR.M2").unwrap();
    let importes: Vec<Decimal> = l.iter().map(|x| x.importe).collect();
    // 9,975→9,98 · 7,70 · 0,24 · 6,125→6,13 · 5,25 · 2 % × 29,30 = 0,586→0,59
    assert_eq!(
        importes,
        vec![dec!(9.98), dec!(7.70), dec!(0.24), dec!(6.13), dec!(5.25), dec!(0.59)]
    );
    assert_eq!(
        l[5].precio,
        dec!(29.30),
        "base del porcentaje = suma de líneas anteriores"
    );
    assert_eq!(p.precio("SR.M2").unwrap(), dec!(29.89));
    assert_eq!(p.precio("SR.COL8").unwrap(), dec!(242.75));
}

#[test]
fn mediciones_e_importes_de_capitulo() {
    let p = suelo_radiante();
    let cap = p.lineas_valoradas("C01").unwrap();
    assert_eq!(cap[0].cantidad, dec!(210.00));
    assert_eq!(cap[0].importe, dec!(6276.90));
    assert_eq!(cap[1].cantidad, dec!(3));
    assert_eq!(cap[1].importe, dec!(728.25));
    assert_eq!(p.pem().unwrap(), dec!(7005.15));
}

#[test]
fn explosion_de_recursos_y_descuadre_de_redondeo() {
    let p = suelo_radiante();
    let e = p.explosion_recursos().unwrap();
    let r = |c| e.recurso(c).unwrap();
    assert_eq!(r("MO.OF1F").cantidad, dec!(57.0)); // 52,5 + 4,5 h
    assert_eq!(r("MO.OF1F").importe, dec!(1396.50));
    assert_eq!(r("MO.AYUF").cantidad, dec!(55.5)); // 52,5 + 3 h
    assert_eq!(r("MO.AYUF").importe, dec!(1165.50));
    assert_eq!(r("MT.PANEL").cantidad, dec!(220.5));
    assert_eq!(r("MT.TUBO16").cantidad, dec!(1470));
    assert_eq!(r("%MA").importe, dec!(123.90));
    let horas: Decimal = e.por_naturaleza(Naturaleza::ManoObra).map(|x| x.cantidad).sum();
    assert_eq!(horas, dec!(112.5));
    assert_eq!(e.total, dec!(7003.05));
    // Descuadre explicado: (9,98 − 9,975) × 210 + (6,13 − 6,125) × 210 = 2,10 €
    assert_eq!(e.descuadre_redondeo, dec!(2.10));
}

#[test]
fn ofertas_con_distinta_unidad_de_contratacion() {
    let p = suelo_radiante();
    let pem_antes = p.pem().unwrap();
    let [a, b, c] = ofertas_montaje_suelo_radiante();

    let ra = simular(&p, std::slice::from_ref(&a)).unwrap();
    assert_eq!(ra.coste_retirado, dec!(2389.80)); // 210 × (6,13 + 5,25)
    assert_eq!(ra.coste_contratado, dec!(1890.00)); // 210 m² × 9,00
    assert_eq!(ra.pem_escenario, dec!(6505.35));
    assert_eq!(ra.ahorro, dec!(499.80));
    assert_eq!(ra.horas_liberadas["MO.OF1F"], dec!(52.5));
    assert_eq!(ra.horas_liberadas["MO.AYUF"], dec!(52.5));
    assert_eq!(ra.notas.len(), 1, "debe avisar de los medios auxiliares");

    let rb = simular(&p, std::slice::from_ref(&b)).unwrap();
    assert_eq!(rb.detalle[0].cantidad_contratada, dec!(1470.00)); // 210 × 7 m/m²
    assert_eq!(rb.coste_contratado, dec!(2352.00));
    assert_eq!(rb.pem_escenario, dec!(6967.35));

    let rc = simular(&p, std::slice::from_ref(&c)).unwrap();
    assert_eq!(rc.coste_contratado, dec!(2300.00));
    assert_eq!(rc.pem_escenario, dec!(6915.35));

    // Comparación homogénea en €/m² de partida
    assert_eq!(ra.precio_equivalente("SUB-A", "SR.M2", 2), Some(dec!(9.00)));
    assert_eq!(rb.precio_equivalente("SUB-B", "SR.M2", 2), Some(dec!(11.20)));
    assert_eq!(rc.precio_equivalente("SUB-C", "SR.M2", 2), Some(dec!(10.95)));

    // El presupuesto original no cambia
    assert_eq!(p.pem().unwrap(), pem_antes);
    assert_eq!(p, suelo_radiante());
    // Los colectores siguen con su mano de obra propia
    assert_eq!(p.precio("SR.COL8").unwrap(), dec!(242.75));
}

#[test]
fn no_se_duplican_costes_entre_paquetes() {
    let p = suelo_radiante();
    let [a, b, _] = ofertas_montaje_suelo_radiante();
    match simular(&p, &[a, b]) {
        Err(ErrorMotor::DobleSustitucion {
            recurso,
            paquete_previo,
            ..
        }) => {
            assert_eq!(recurso, "MO.OF1F");
            assert_eq!(paquete_previo, "SUB-A");
        }
        otro => panic!("se esperaba DobleSustitucion, se obtuvo {otro:?}"),
    }
}

#[test]
fn paquetes_complementarios_si_se_pueden_combinar() {
    // Oficial subcontratado a 5,00 €/m² y ayudante a 4,00 €/m²: no se solapan.
    let p = suelo_radiante();
    let paq = |cod: &str, rec: &str, precio| PaqueteTrabajo {
        codigo: cod.into(),
        descripcion: cod.into(),
        contratacion: Contratacion::PorUnidad {
            unidad: "m2".into(),
            precio,
        },
        asignaciones: vec![Asignacion {
            partida: "SR.M2".into(),
            recursos: vec![rec.into()],
            factor_conversion: dec!(1),
        }],
        incluye: vec![],
        excluye: vec![],
    };
    let r = simular(
        &p,
        &[paq("OF", "MO.OF1F", dec!(5.00)), paq("AY", "MO.AYUF", dec!(4.00))],
    )
    .unwrap();
    assert_eq!(r.coste_retirado, dec!(2389.80));
    assert_eq!(r.coste_contratado, dec!(1890.00));
}

#[test]
fn recurso_ajeno_a_la_partida() {
    let p = suelo_radiante();
    let paq = PaqueteTrabajo {
        codigo: "X".into(),
        descripcion: "x".into(),
        contratacion: Contratacion::Alzado { importe: dec!(1) },
        asignaciones: vec![Asignacion {
            partida: "SR.M2".into(),
            recursos: vec!["MT.COL8".into()],
            factor_conversion: dec!(0),
        }],
        incluye: vec![],
        excluye: vec![],
    };
    assert!(matches!(
        simular(&p, &[paq]),
        Err(ErrorMotor::RecursoNoEnPartida { .. })
    ));
}

#[test]
fn resumen_y_margen_sobre_el_ejemplo() {
    let p = suelo_radiante();
    let r = venta::resumen(
        p.pem().unwrap(),
        venta::GG_HABITUAL,
        venta::BI_HABITUAL,
        venta::IVA_GENERAL,
        2,
    );
    assert_eq!(r.total, dec!(10086.72));
    // Venta con 30 % de margen sobre el coste directo del suelo radiante
    assert_eq!(venta::venta_por_margen(dec!(29.89), dec!(30), 2).unwrap(), dec!(42.70));
}
