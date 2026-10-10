// SPDX-License-Identifier: GPL-3.0-or-later
//! Presupuesto sintético grande: comprueba que la valoración en bloque
//! coincide con la valoración unitaria y que escala a miles de partidas.

use ppto_core::concepto::{Concepto, LineaDescomposicion as L, Naturaleza};
use ppto_core::ejemplos::suelo_radiante;
use ppto_core::{Decimal, Presupuesto};
use std::time::Instant;

/// 40 capítulos × 100 partidas, cada una con 6 recursos de un catálogo de 500
/// y un precio auxiliar compartido.
fn grande() -> Presupuesto {
    let mut p = Presupuesto::new("grande", "OBRA", "Obra sintética grande");
    for r in 0..500u32 {
        let nat = [Naturaleza::Material, Naturaleza::ManoObra, Naturaleza::Maquinaria][(r % 3) as usize];
        let precio = Decimal::new(i64::from(100 + r * 7 % 900), 2);
        p.insertar(Concepto::basico(
            format!("R{r:04}"),
            "ud",
            format!("Recurso {r}"),
            nat,
            precio,
        ))
        .unwrap();
    }
    p.insertar(Concepto::partida(
        "AUX",
        "m3",
        "Auxiliar",
        vec![
            L::new("R0001", Decimal::new(250, 3)),
            L::new("R0002", Decimal::new(1100, 3)),
        ],
    ))
    .unwrap();
    for c in 0..40u32 {
        let cap = format!("C{c:02}");
        p.insertar(Concepto::capitulo(cap.clone(), format!("Capítulo {c}")))
            .unwrap();
        p.colgar("OBRA", &cap, None).unwrap();
        for i in 0..100u32 {
            let n = c * 100 + i;
            let mut lineas: Vec<L> = (0..6u32)
                .map(|k| {
                    L::new(
                        format!("R{:04}", (n * 7 + k * 31) % 500),
                        Decimal::new(i64::from(1 + (n + k) % 9), 1),
                    )
                })
                .collect();
            lineas.push(L::new("AUX", Decimal::new(20, 3)));
            let cod = format!("P{n:05}");
            p.insertar(Concepto::partida(cod.clone(), "m2", format!("Partida {n}"), lineas))
                .unwrap();
            p.colgar(&cap, &cod, None).unwrap();
            // cantidad de la partida en el capítulo
            p.fijar_rendimiento(&cap, &cod, Decimal::new(i64::from(1 + n % 50), 0))
                .unwrap();
        }
    }
    p
}

#[test]
fn valoracion_en_bloque_coincide_con_unitaria() {
    let p = suelo_radiante();
    let v = p.valorar().unwrap();
    assert_eq!(v.pem, p.pem().unwrap());
    for codigo in p.conceptos.keys() {
        assert_eq!(v.precios[codigo], p.precio(codigo).unwrap(), "{codigo}");
    }
    assert_eq!(v.lineas["SR.M2"], p.lineas_valoradas("SR.M2").unwrap());
}

#[test]
fn escala_a_miles_de_partidas() {
    let p = grande();
    assert_eq!(p.conceptos.len(), 500 + 1 + 40 + 4000 + 1);
    let t = Instant::now();
    let v = p.valorar().unwrap();
    let e = p.explosion_recursos().unwrap();
    let ms = t.elapsed().as_millis();
    assert_eq!(v.pem, p.pem().unwrap());
    // Cota del descuadre: cada partida redondea 7 importes de línea y su precio
    // (±0,005 € cada uno) y el error se multiplica por su cantidad en obra;
    // además, cada importe de partida, de capítulo y de recurso redondea ±0,005 €.
    let medio = Decimal::new(5, 3);
    let cota: Decimal = p
        .cantidades_partidas()
        .unwrap()
        .values()
        .map(|q| *q * medio * Decimal::from(8))
        .sum::<Decimal>()
        + medio * Decimal::from(4000 + 40 + 501);
    assert!(
        e.descuadre_redondeo.abs() <= cota,
        "descuadre {} > cota {}",
        e.descuadre_redondeo,
        cota
    );
    eprintln!("descuadre {} € (cota {} €)", e.descuadre_redondeo, cota);
    eprintln!("4.000 partidas valoradas y explotadas en {ms} ms");
}
