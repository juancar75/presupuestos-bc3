// SPDX-License-Identifier: GPL-3.0-or-later
//! Comparación de dos versiones de un presupuesto (p. ej. revisión R1 frente
//! a R2): partidas nuevas, quitadas y cambiadas (cantidad o precio), con la
//! diferencia de importe de cada una y del PEM.
//!
//! Las partidas se identifican por (capítulo, código). Una partida que pasa
//! de un capítulo a otro aparece como quitada en uno y nueva en el otro.

use crate::error::ErrorMotor;
use crate::presupuesto::{Presupuesto, Valoracion};
use crate::{Decimal, Naturaleza};
use std::collections::BTreeMap;

/// Cantidad, precio e importe de una partida en un capítulo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Valor {
    pub cantidad: Decimal,
    pub precio: Decimal,
    pub importe: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estado {
    Nueva,
    Quitada,
    Cambiada,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CambioPartida {
    pub estado: Estado,
    pub capitulo: String,
    pub codigo: String,
    pub unidad: String,
    pub resumen: String,
    pub antes: Option<Valor>,
    pub despues: Option<Valor>,
}

impl CambioPartida {
    /// Diferencia de importe (después − antes).
    pub fn diferencia(&self) -> Decimal {
        self.despues.map_or(Decimal::ZERO, |v| v.importe) - self.antes.map_or(Decimal::ZERO, |v| v.importe)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparacion {
    pub cambios: Vec<CambioPartida>,
    pub pem_antes: Decimal,
    pub pem_despues: Decimal,
    /// Partidas iguales en las dos versiones.
    pub iguales: usize,
}

impl Comparacion {
    pub fn diferencia(&self) -> Decimal {
        self.pem_despues - self.pem_antes
    }

    /// Variación del PEM en % sobre el de antes (`None` si antes era 0).
    pub fn variacion_pct(&self) -> Option<Decimal> {
        (!self.pem_antes.is_zero()).then(|| (self.diferencia() / self.pem_antes * Decimal::ONE_HUNDRED).round_dp(2))
    }
}

/// Partidas de un presupuesto con su valor, en orden de presupuesto.
fn partidas(p: &Presupuesto, v: &Valoracion) -> Vec<((String, String), Valor)> {
    fn rec(p: &Presupuesto, v: &Valoracion, cap: &str, out: &mut Vec<((String, String), Valor)>) {
        for l in v.lineas.get(cap).into_iter().flatten() {
            let es_cap = p
                .conceptos
                .get(&l.hijo)
                .is_some_and(|c| c.naturaleza == Naturaleza::Capitulo);
            if es_cap {
                rec(p, v, &l.hijo, out);
            } else {
                out.push((
                    (cap.to_owned(), l.hijo.clone()),
                    Valor {
                        cantidad: l.cantidad,
                        precio: l.precio,
                        importe: l.importe,
                    },
                ));
            }
        }
    }
    let mut out = Vec::new();
    rec(p, v, &p.raiz, &mut out);
    out
}

/// Compara `antes` con `despues`.
pub fn comparar(antes: &Presupuesto, despues: &Presupuesto) -> Result<Comparacion, ErrorMotor> {
    let va = antes.valorar()?;
    let vd = despues.valorar()?;
    let pa = partidas(antes, &va);
    let pd = partidas(despues, &vd);
    let mapa_a: BTreeMap<_, _> = pa.iter().cloned().collect();
    let mapa_d: BTreeMap<_, _> = pd.iter().cloned().collect();
    let datos = |p: &Presupuesto, codigo: &str| {
        p.conceptos
            .get(codigo)
            .map(|c| (c.unidad.clone(), c.resumen.clone()))
            .unwrap_or_default()
    };
    let mut cambios = Vec::new();
    let mut iguales = 0;
    for (k, d) in &pd {
        let (unidad, resumen) = datos(despues, &k.1);
        match mapa_a.get(k) {
            None => cambios.push(CambioPartida {
                estado: Estado::Nueva,
                capitulo: k.0.clone(),
                codigo: k.1.clone(),
                unidad,
                resumen,
                antes: None,
                despues: Some(*d),
            }),
            Some(a) if a.cantidad != d.cantidad || a.precio != d.precio => cambios.push(CambioPartida {
                estado: Estado::Cambiada,
                capitulo: k.0.clone(),
                codigo: k.1.clone(),
                unidad,
                resumen,
                antes: Some(*a),
                despues: Some(*d),
            }),
            Some(_) => iguales += 1,
        }
    }
    for (k, a) in &pa {
        if !mapa_d.contains_key(k) {
            let (unidad, resumen) = datos(antes, &k.1);
            cambios.push(CambioPartida {
                estado: Estado::Quitada,
                capitulo: k.0.clone(),
                codigo: k.1.clone(),
                unidad,
                resumen,
                antes: Some(*a),
                despues: None,
            });
        }
    }
    Ok(Comparacion {
        cambios,
        pem_antes: va.pem,
        pem_despues: vd.pem,
        iguales,
    })
}
