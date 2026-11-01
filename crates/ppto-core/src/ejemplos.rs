// SPDX-License-Identifier: GPL-3.0-or-later
//! Presupuestos sintéticos de referencia.
//!
//! **Datos ficticios** creados para pruebas (regla 5 y 10 del proyecto): los
//! precios son orientativos y no proceden de ningún cliente ni tarifa real.
//! Los valores esperados están calculados a mano en
//! `docs/casos-validacion/suelo-radiante.md`.

use crate::concepto::{Concepto, LineaDescomposicion as L, Naturaleza};
use crate::medicion::{LineaMedicion, Medicion};
use crate::presupuesto::Presupuesto;
use crate::subcontrata::{Asignacion, Contratacion, PaqueteTrabajo};
use rust_decimal_macros::dec;

/// Vivienda tipo con suelo radiante: un capítulo con la partida de suelo
/// radiante (m²) y otra de colectores (ud) que comparte mano de obra.
pub fn suelo_radiante() -> Presupuesto {
    let mut p = Presupuesto::new(
        "Ejemplo sintético — suelo radiante",
        "OBRA",
        "Vivienda tipo (sintético)",
    );
    let b = |c: &str, u: &str, r: &str, n, pr| Concepto::basico(c, u, r, n, pr);
    for c in [
        b(
            "MO.OF1F",
            "h",
            "Oficial 1ª instalador de calefacción",
            Naturaleza::ManoObra,
            dec!(24.50),
        ),
        b(
            "MO.AYUF",
            "h",
            "Ayudante instalador de calefacción",
            Naturaleza::ManoObra,
            dec!(21.00),
        ),
        b(
            "MT.PANEL",
            "m2",
            "Panel aislante de tetones EPS 30 mm",
            Naturaleza::Material,
            dec!(9.50),
        ),
        b(
            "MT.TUBO16",
            "m",
            "Tubo PE-RT 16×2 mm con barrera de oxígeno",
            Naturaleza::Material,
            dec!(1.10),
        ),
        b(
            "MT.BANDA",
            "m",
            "Banda perimetral de espuma",
            Naturaleza::Material,
            dec!(0.60),
        ),
        b(
            "MT.COL8",
            "ud",
            "Colector de 8 vías con caudalímetros",
            Naturaleza::Material,
            dec!(185.00),
        ),
        Concepto::porcentaje("%MA", "Medios auxiliares"),
    ] {
        p.insertar(c).expect("código único");
    }
    p.insertar(Concepto::partida(
        "SR.M2",
        "m2",
        "Suelo radiante con panel de tetones y tubo PE-RT 16×2",
        vec![
            L::new("MT.PANEL", dec!(1.050)),
            L::new("MT.TUBO16", dec!(7.000)),
            L::new("MT.BANDA", dec!(0.400)),
            L::new("MO.OF1F", dec!(0.250)),
            L::new("MO.AYUF", dec!(0.250)),
            L::new("%MA", dec!(2.000)),
        ],
    ))
    .expect("código único");
    p.insertar(Concepto::partida(
        "SR.COL8",
        "ud",
        "Colector de 8 vías instalado y conexionado",
        vec![
            L::new("MT.COL8", dec!(1)),
            L::new("MO.OF1F", dec!(1.500)),
            L::new("MO.AYUF", dec!(1.000)),
        ],
    ))
    .expect("código único");
    p.insertar(Concepto::capitulo("C01", "Calefacción por suelo radiante"))
        .expect("código único");
    for (codigo, texto) in [
        (
            "SR.M2",
            "Sistema de calefacción por suelo radiante formado por panel aislante de poliestireno \
             expandido con tetones de 30 mm, tubo de polietileno reticulado PE-RT 16×2 mm con \
             barrera de oxígeno con paso medio de 15 cm y banda perimetral de espuma. Incluso \
             replanteo, fijación, prueba de estanqueidad y limpieza. Medida la superficie útil \
             calefactada.",
        ),
        (
            "SR.COL8",
            "Colector de 8 vías con caudalímetros, detentores, purgadores y llaves de corte, \
             instalado en armario y conexionado a los circuitos. Totalmente probado.",
        ),
    ] {
        p.conceptos.get_mut(codigo).expect("existe").texto = Some(texto.into());
    }
    p.colgar("OBRA", "C01", None).expect("existe");

    let n = |c: &str, u, l, a| LineaMedicion::normal(c, Some(u), Some(l), Some(a), None);
    p.colgar(
        "C01",
        "SR.M2",
        Some(Medicion::new(vec![
            n("Planta baja", dec!(1), dec!(12.50), dec!(8.00)),
            n("Planta primera", dec!(1), dec!(12.50), dec!(8.00)),
            n("Baños", dec!(2), dec!(3.20), dec!(2.50)),
            n("A deducir hueco de escalera", dec!(-1), dec!(2.00), dec!(3.00)),
        ])),
    )
    .expect("existe");
    p.colgar(
        "C01",
        "SR.COL8",
        Some(Medicion::new(vec![
            LineaMedicion::normal("Planta baja", Some(dec!(2)), None, None, None),
            LineaMedicion::normal("Planta primera", Some(dec!(1)), None, None, None),
        ])),
    )
    .expect("existe");
    p
}

/// Tres ofertas de subcontrata para el montaje del suelo radiante (solo mano
/// de obra), cada una con una unidad de contratación distinta.
pub fn ofertas_montaje_suelo_radiante() -> [PaqueteTrabajo; 3] {
    let asig = |factor| {
        vec![Asignacion {
            partida: "SR.M2".into(),
            recursos: vec!["MO.OF1F".into(), "MO.AYUF".into()],
            factor_conversion: factor,
        }]
    };
    let incluye = vec![
        "Montaje de panel, tubo y banda perimetral".into(),
        "Prueba de estanqueidad".into(),
    ];
    let excluye = vec!["Suministro de materiales".into(), "Colectores".into()];
    [
        PaqueteTrabajo {
            codigo: "SUB-A".into(),
            descripcion: "Montaje por m² de superficie radiante".into(),
            contratacion: Contratacion::PorUnidad {
                unidad: "m2".into(),
                precio: dec!(9.00),
            },
            asignaciones: asig(dec!(1)),
            incluye: incluye.clone(),
            excluye: excluye.clone(),
        },
        PaqueteTrabajo {
            codigo: "SUB-B".into(),
            descripcion: "Montaje por metro lineal de tubo".into(),
            contratacion: Contratacion::PorUnidad {
                unidad: "m".into(),
                precio: dec!(1.60),
            },
            asignaciones: asig(dec!(7)),
            incluye: incluye.clone(),
            excluye: excluye.clone(),
        },
        PaqueteTrabajo {
            codigo: "SUB-C".into(),
            descripcion: "Montaje a precio cerrado".into(),
            contratacion: Contratacion::Alzado { importe: dec!(2300.00) },
            asignaciones: asig(dec!(0)),
            incluye,
            excluye,
        },
    ]
}

/// Presupuesto sintético **aleatorio pero repetible** (misma `semilla`, mismo
/// presupuesto) para pruebas de integridad: capítulos y subcapítulos,
/// partidas descompuestas y de precio directo, auxiliares, %MA, factores,
/// partidas repetidas en varios capítulos, mediciones de todos los tipos,
/// tarifas con grupo y descuentos, y costes indirectos.
///
/// Con `extras`, también los datos propios del programa que no van al BC3:
/// margen por partida, venta bloqueada y origen del precio.
pub fn aleatorio(semilla: u64, extras: bool) -> Presupuesto {
    use crate::Decimal;
    use crate::medicion::TipoLinea;
    // xorshift64*: suficiente para pruebas y sin dependencias
    let mut x = semilla.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut r = move |n: u64| {
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33) % n.max(1)
    };
    let d = |v: u64, escala: u32| Decimal::new(i64::try_from(v).unwrap_or(0), escala);
    let textos = [
        "Instalación según RITE",
        "Montaje con p.p. de soportes, «piezas especiales» y pruebas",
        "Línea 1\nLínea 2 con ñ, á, é, ü y 50 %",
        "Equipo 3,5 kW — 230 V",
    ];
    let mut p = Presupuesto::new(
        format!("Aleatorio {semilla}"),
        "OBRA",
        format!("Obra aleatoria {semilla}"),
    );
    if r(3) == 0 {
        p.costes_indirectos = d(r(6) + 1, 0);
        p.opciones_ci.redondear_coste_antes = r(2) == 0;
        p.opciones_ci.aplicar_a_sin_descomponer = r(2) == 0;
    }
    p.opciones_ci.redondear_auxiliares = r(2) == 0;
    // Recursos básicos
    let nat = [
        (Naturaleza::ManoObra, "MO", "h"),
        (Naturaleza::Material, "MT", "ud"),
        (Naturaleza::Maquinaria, "MQ", "h"),
        (Naturaleza::Subcontrata, "SB", "m2"),
        (Naturaleza::Otros, "OT", "ud"),
    ];
    let mut basicos = Vec::new();
    for i in 0..(4 + r(7)) {
        let (n, pre, u) = nat[r(5) as usize];
        let codigo = format!("{pre}{i:02}");
        let resumen = format!("{} {i} {}", pre, textos[r(4) as usize].lines().next().unwrap_or(""));
        let mut c = Concepto::basico(&codigo, u, resumen, n, d(r(20_000) + 1, 2));
        if r(4) == 0 {
            c.texto = Some(textos[r(4) as usize].to_owned());
        }
        p.insertar(c).ok();
        basicos.push(codigo);
    }
    // Materiales de tarifa con descuento general o por grupo
    if r(2) == 0 {
        p.fijar_descuento("FABA", d(r(60), 0)).ok();
        p.fijar_descuento("FABA/G1", d(r(70), 0)).ok();
        for (i, codigo) in basicos.iter().filter(|c| c.starts_with("MT")).enumerate() {
            let grupo = (i % 2 == 0).then_some("G1");
            p.fijar_tarifa_grupo(codigo, "FABA", grupo, d(r(50_000) + 100, 2)).ok();
        }
    }
    p.insertar(Concepto::porcentaje("%MA", "Medios auxiliares")).ok();
    let lineas = |r: &mut dyn FnMut(u64) -> u64, base: &[String]| -> Vec<L> {
        let mut v: Vec<L> = Vec::new();
        for _ in 0..(1 + r(4)) {
            let h = &base[r(base.len() as u64) as usize];
            if v.iter().any(|l| &l.hijo == h) {
                continue;
            }
            let mut l = L::new(h, d(r(5000) + 1, 3));
            if r(5) == 0 {
                l.factor = d(r(3) + 1, 1);
            }
            v.push(l);
        }
        v
    };
    // Auxiliares (subpartidas usadas dentro de otras)
    let mut auxiliares = Vec::new();
    for i in 0..r(3) {
        let codigo = format!("AUX{i}");
        let ls = lineas(&mut r, &basicos);
        p.insertar(Concepto::partida(&codigo, "ud", format!("Auxiliar {i}"), ls))
            .ok();
        auxiliares.push(codigo);
    }
    let mut usables = basicos.clone();
    usables.extend(auxiliares.iter().cloned());
    // Capítulos, subcapítulos y partidas
    let raiz = p.raiz.clone();
    let mut partidas: Vec<(String, String)> = Vec::new();
    let mut n_partida = 0;
    for c in 0..(1 + r(3)) {
        let cap = format!("C{:02}", c + 1);
        p.nuevo_capitulo(
            &raiz,
            &cap,
            &format!(
                "Capítulo {} {}",
                c + 1,
                textos[r(4) as usize].lines().next().unwrap_or("")
            ),
        )
        .ok();
        let mut destinos = vec![cap.clone()];
        for s in 0..r(3) {
            let sub = format!("{cap}.{:02}", s + 1);
            p.nuevo_capitulo(&cap, &sub, &format!("Subcapítulo {sub}")).ok();
            destinos.push(sub);
        }
        for dest in destinos {
            for _ in 0..(1 + r(4)) {
                // A veces se repite una partida ya creada en otro capítulo
                if r(6) == 0
                    && let Some((_, ya)) = partidas.get(r(partidas.len() as u64 + 1) as usize).cloned()
                    && p.anadir_linea(&dest, &ya, d(r(900) + 1, 1)).is_ok()
                {
                    continue;
                }
                n_partida += 1;
                let codigo = format!("P{n_partida:03}");
                let mut ls = if r(5) == 0 {
                    Vec::new()
                } else {
                    lineas(&mut r, &usables)
                };
                if !ls.is_empty() && r(2) == 0 {
                    ls.push(L::new("%MA", d(r(5) + 1, 0)));
                }
                let directa = ls.is_empty();
                let mut c = Concepto::partida(
                    &codigo,
                    ["ud", "m", "m2", "PA"][r(4) as usize],
                    format!("Partida {n_partida}"),
                    ls,
                );
                if directa {
                    c.precio = d(r(100_000) + 1, 2);
                }
                if r(2) == 0 {
                    c.texto = Some(textos[r(4) as usize].to_owned());
                }
                p.insertar(c).ok();
                p.anadir_linea(&dest, &codigo, Decimal::ONE).ok();
                // Medición: unas veces cantidad directa, otras con líneas
                if r(3) == 0 {
                    p.fijar_rendimiento(&dest, &codigo, d(r(10_000) + 1, 2)).ok();
                } else {
                    let mut m = Vec::new();
                    for k in 0..(1 + r(4)) {
                        let opt = |r: &mut dyn FnMut(u64) -> u64, esc| (r(3) > 0).then(|| d(r(2000) + 1, esc));
                        let tipo = match r(8) {
                            0 if k > 0 => TipoLinea::SubtotalParcial,
                            1 if k > 0 => TipoLinea::SubtotalAcumulado,
                            _ => TipoLinea::Normal,
                        };
                        let mut l = LineaMedicion::normal(
                            &format!("Planta {k}"),
                            opt(&mut r, 2),
                            opt(&mut r, 2),
                            opt(&mut r, 2),
                            opt(&mut r, 3),
                        );
                        if tipo != TipoLinea::Normal {
                            l = LineaMedicion {
                                tipo,
                                ..LineaMedicion::normal("Subtotal", None, None, None, None)
                            };
                        }
                        m.push(l);
                    }
                    p.actualizar_medicion(&dest, &codigo, Medicion::new(m)).ok();
                }
                partidas.push((dest.clone(), codigo));
            }
        }
    }
    if extras {
        for (_, c) in &partidas {
            match r(4) {
                0 => {
                    p.fijar_margen(c, Some(d(r(40) + 5, 0))).ok();
                }
                1 => {
                    p.fijar_venta(c, Some(d(r(100_000) + 1, 2))).ok();
                }
                _ => {}
            }
        }
        for b in &basicos {
            if r(3) != 0 {
                continue;
            }
            p.fijar_origen(
                b,
                Some(crate::OrigenPrecio {
                    fabricante: "PROVEEDOR".into(),
                    oferta: format!("OF-{}", r(999)),
                    fecha: "2026-10-01".into(),
                    validez: (r(2) == 0).then(|| "2026-12-31".to_owned()),
                    referencia: Some(format!("REF-{}", r(9999))),
                    neto: Some(d(r(9999) + 1, 2)),
                    documento: Some(r"\\servidor\obras\Oferta «A» 1.pdf".into()),
                }),
                false,
            )
            .ok();
        }
        // Seguimiento: un contradictorio y certificaciones (al final, para no
        // cambiar el resto de semillas)
        if r(2) == 0 {
            seguimiento_aleatorio(&mut p, &mut r);
        }
    }
    p
}

fn seguimiento_aleatorio(p: &mut Presupuesto, r: &mut impl FnMut(u64) -> u64) {
    use crate::Decimal;
    // Pares (capítulo, partida) del árbol
    let pares: Vec<(String, String)> = p
        .conceptos
        .values()
        .filter(|c| c.naturaleza == Naturaleza::Capitulo)
        .flat_map(|c| {
            c.descomposicion
                .iter()
                .filter(|l| {
                    p.conceptos
                        .get(&l.hijo)
                        .is_some_and(|h| h.naturaleza != Naturaleza::Capitulo)
                })
                .map(|l| (c.codigo.clone(), l.hijo.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    if let Ok(ctr) = p.nuevo_contradictorio("Unidades no previstas", "15/10/2026", "Orden de la DF nº 3") {
        if let Some((_, partida)) = pares.first() {
            p.colgar(&ctr, partida, None).ok();
        }
        if r(2) == 0 {
            p.fijar_estado_contradictorio(&ctr, crate::seguimiento::EstadoContradictorio::Aprobado)
                .ok();
        }
    }
    for n in 0..r(3) + 1 {
        let Ok(num) = p.nueva_certificacion(&format!("{}/2026", 10 + n)) else {
            break;
        };
        for (cap, partida) in &pares {
            if r(2) == 0 {
                p.certificar_mes(num, cap, partida, Decimal::new(i64::try_from(r(500)).unwrap_or(0), 2))
                    .ok();
            }
        }
        if r(3) != 0 {
            p.cerrar_certificacion(num, Decimal::from(13), Decimal::from(6)).ok();
        } else {
            break;
        }
    }
    // Pedidos registrados: uno enviado y en parte recibido
    if let Ok(pedidos) = crate::pedidos::pedidos(p)
        && let Some(x) = pedidos.first()
        && let Some(n) = p.registrar_pedido(x, "20/10/2026", "OBRA")
    {
        p.fijar_estado_pedido(n, crate::compras::EstadoPedido::Enviado).ok();
        if let Some(l) = p.pedido_registrado(n).and_then(|x| x.lineas.first()).cloned() {
            p.recibir(n, &l.codigo, l.cantidad / Decimal::TWO).ok();
        }
    }
}
