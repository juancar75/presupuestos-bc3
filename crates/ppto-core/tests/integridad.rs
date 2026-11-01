// SPDX-License-Identifier: GPL-3.0-or-later
//! Integridad de los datos con presupuestos aleatorios (repetibles): lo que
//! se guarda, se copia o se calcula vuelve igual y cuadra.

use ppto_core::Decimal;
use ppto_core::ejemplos::aleatorio;
use ppto_core::fragmento::{Fragmento, copiar, pegar};
use ppto_core::pedidos::{coste_obra, mano_obra, pedidos};
use ppto_core::{Naturaleza, Presupuesto};
use rust_decimal_macros::dec;

const SEMILLAS: u64 = 300;

#[test]
fn aleatorios_validos_y_valorables() {
    for s in 0..SEMILLAS {
        for extras in [false, true] {
            let p = aleatorio(s, extras);
            p.validar().unwrap_or_else(|e| panic!("semilla {s}: {e}"));
            let v = p.valorar().unwrap();
            assert!(v.pem >= Decimal::ZERO, "semilla {s}");
            // Repetible
            assert_eq!(aleatorio(s, extras), p, "semilla {s}");
        }
    }
}

#[test]
fn json_ida_y_vuelta() {
    // Es el formato de copiar/pegar entre obras y de la obra activa para la IA
    for s in 0..SEMILLAS {
        let p = aleatorio(s, true);
        let t = serde_json::to_string(&p).unwrap();
        let q: Presupuesto = serde_json::from_str(&t).unwrap();
        assert_eq!(q, p, "semilla {s}");
    }
}

#[test]
fn explosion_pedidos_y_coste_cuadran() {
    for s in 0..SEMILLAS {
        let p = aleatorio(s, true);
        let e = p.explosion_recursos().unwrap();
        // La explosión cuadra con el coste directo salvo el redondeo: cada
        // línea redondea su importe por unidad (±0,005 €) y la explosión lo
        // multiplica por la cantidad de la partida; más ±0,005 € por partida.
        let tol: Decimal = p
            .cantidades_partidas()
            .unwrap()
            .iter()
            .map(|(c, q)| q.abs() * error_redondeo(&p, c) + dec!(0.005))
            .sum::<Decimal>()
            + dec!(0.01);
        assert!(
            e.descuadre_redondeo.abs() <= tol,
            "semilla {s}: descuadre {}",
            e.descuadre_redondeo
        );
        // Todo el material, maquinaria, otros y subcontrata sale en algún pedido
        let comprable: Decimal = e
            .recursos
            .iter()
            .filter(|r| {
                !matches!(
                    r.naturaleza,
                    Naturaleza::ManoObra | Naturaleza::Porcentaje | Naturaleza::Partida
                )
            })
            .filter(|r| !r.cantidad.is_zero())
            .map(|r| r.importe)
            .sum();
        let en_pedidos: Decimal = pedidos(&p).unwrap().iter().map(|x| x.total).sum();
        assert_eq!(en_pedidos, comprable, "semilla {s}");
        // Mano de obra por capítulos = total
        let mo = mano_obra(&p).unwrap();
        // (cada capítulo redondea su importe: como mucho un céntimo por capítulo y oficio)
        let por_cap: Decimal = mo.capitulos.iter().map(|c| c.importe).sum();
        let lineas: usize = mo.capitulos.iter().map(|c| c.oficios.len()).sum();
        let tol = Decimal::new(i64::try_from(lineas).unwrap(), 2);
        assert!(
            (por_cap - mo.importe).abs() <= tol,
            "semilla {s}: {por_cap} ≠ {}",
            mo.importe
        );
        // Coste por capítulos = PEM
        let c = coste_obra(&p, dec!(13), dec!(6)).unwrap();
        let suma: Decimal = c.capitulos.iter().map(|x| x.coste).sum();
        assert_eq!(suma, c.pem, "semilla {s}");
        assert_eq!(c.pem, p.pem().unwrap(), "semilla {s}");
    }
}

#[test]
fn copiar_y_pegar_conserva_importes() {
    for s in 0..SEMILLAS {
        let p = aleatorio(s, true);
        let caps: Vec<String> = p.conceptos[&p.raiz]
            .descomposicion
            .iter()
            .map(|l| l.hijo.clone())
            .collect();
        let f = copiar(&p, &p.raiz, &caps).unwrap();
        let f = Fragmento::de_texto(&f.a_texto()).expect("el texto del portapapeles se vuelve a leer");
        let mut q = Presupuesto::new("destino", "DEST", "Obra destino");
        q.costes_indirectos = p.costes_indirectos;
        q.opciones_ci = p.opciones_ci;
        let inf = pegar(&mut q, &f, "DEST").unwrap();
        assert!(inf.renombrados.is_empty(), "semilla {s}");
        assert_eq!(q.pem().unwrap(), p.pem().unwrap(), "semilla {s}");
        // Pegar otra vez en la misma obra (en otro capítulo): no se pisa nada
        // y el importe se duplica
        let pem1 = q.pem().unwrap();
        q.nuevo_capitulo("DEST", "COPIA", "Copia").unwrap();
        pegar(&mut q, &f, "COPIA").unwrap();
        assert_eq!(q.pem().unwrap(), pem1 + pem1, "semilla {s}");
    }
}

/// Error de redondeo máximo por unidad de una partida: ±0,005 € por línea y
/// por su propio precio, más el de cada auxiliar multiplicado por su cantidad.
fn error_redondeo(p: &Presupuesto, codigo: &str) -> Decimal {
    let c = &p.conceptos[codigo];
    let mut e = dec!(0.005);
    for l in &c.descomposicion {
        e += dec!(0.005);
        if p.conceptos[&l.hijo].naturaleza == Naturaleza::Partida {
            e += l.cantidad().abs() * error_redondeo(p, &l.hijo);
        }
    }
    e
}

/// Certificaciones: anterior + mes = origen en cada fila y en los totales, y
/// los subtotales de capítulo suman el total.
#[test]
fn certificaciones_cuadran() {
    use ppto_core::seguimiento::FilaRelacion;
    let mut probadas = 0;
    for s in 0..300 {
        let p = ppto_core::ejemplos::aleatorio(s, true);
        for c in &p.seguimiento.certificaciones {
            probadas += 1;
            for pr in [
                ppto_core::seguimiento::Precios::Ejecucion {
                    gg: dec!(13),
                    bi: dec!(6),
                },
                ppto_core::seguimiento::Precios::Venta {
                    gg: dec!(13),
                    bi: dec!(6),
                },
            ] {
                let r = p.relacion_valorada(c.numero, pr).unwrap();
                assert_eq!(r.total.anterior + r.total.actual, r.total.origen, "semilla {s}");
                let mut suma = Decimal::ZERO;
                for f in &r.filas {
                    match f {
                        FilaRelacion::Partida(f) => {
                            assert_eq!(
                                f.importes.anterior + f.importes.actual,
                                f.importes.origen,
                                "semilla {s}"
                            );
                            assert_eq!(f.cant_anterior + f.cant_actual, f.cant_origen, "semilla {s}");
                        }
                        FilaRelacion::Capitulo { nivel: 0, importes, .. } => suma += importes.origen,
                        FilaRelacion::Capitulo { .. } => {}
                    }
                }
                assert_eq!(suma, r.total.origen, "semilla {s}");
                let res = p.resumen_certificacion(&r, pr, dec!(21), dec!(5));
                assert_eq!(res.anterior.total + res.actual.total, res.origen.total, "semilla {s}");
            }
        }
    }
    assert!(probadas > 50, "{probadas}");
}

/// Control de compras: sin errores y coherente con la explosión.
#[test]
fn control_de_compras_coherente() {
    let mut con_pedidos = 0;
    for s in 0..200 {
        let p = aleatorio(s, true);
        let c = p.control_compras().unwrap();
        let ya = p.ya_pedido();
        for r in &c {
            assert_eq!(r.pedido, ya.get(&r.codigo).copied().unwrap_or_default(), "semilla {s}");
            assert!(r.recibido <= r.pedido, "semilla {s}");
            assert!(r.por_pedir() >= Decimal::ZERO && r.por_recibir() >= Decimal::ZERO);
        }
        con_pedidos += usize::from(!p.seguimiento.pedidos.is_empty());
    }
    assert!(con_pedidos > 20, "{con_pedidos}");
}
