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
