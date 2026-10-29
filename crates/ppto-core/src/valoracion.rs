// SPDX-License-Identifier: GPL-3.0-or-later
//! Estado de valoración de cada partida, para ver de un vistazo en el árbol
//! lo que falta: sin precio (rojo) o con el descompuesto a medias (naranja:
//! mano de obra sin material o material sin mano de obra).

use crate::Decimal;
use crate::concepto::Naturaleza;
use crate::presupuesto::Presupuesto;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Estado {
    /// Con precio y descompuesto completo (o subcontratada, o precio directo).
    Valorada,
    /// Hay mano de obra pero ningún material.
    SoloManoObra,
    /// Hay material pero ninguna mano de obra.
    SoloMaterial,
    /// Precio cero.
    SinPrecio,
}

impl Estado {
    pub fn texto(self) -> &'static str {
        match self {
            Self::Valorada => "valorada",
            Self::SoloManoObra => "solo mano de obra (falta material)",
            Self::SoloMaterial => "solo material (falta mano de obra)",
            Self::SinPrecio => "sin valorar",
        }
    }

    /// Le falta algo al descompuesto (naranja).
    pub fn incompleta(self) -> bool {
        matches!(self, Self::SoloManoObra | Self::SoloMaterial)
    }

    /// El más grave de los dos: sin precio > incompleta > valorada.
    pub fn peor(self, otro: Self) -> Self {
        let g = |e: Self| match e {
            Self::Valorada => 0,
            Self::SoloManoObra | Self::SoloMaterial => 1,
            Self::SinPrecio => 2,
        };
        if g(otro) > g(self) { otro } else { self }
    }
}

/// Estado de una partida según lo que lleva su descompuesto (con sus
/// auxiliares explotados). Una subcontrata cuenta como completa.
pub fn estado(p: &Presupuesto, partida: &str) -> Estado {
    let Ok(c) = p.concepto(partida) else {
        return Estado::SinPrecio;
    };
    if c.descomposicion.is_empty() {
        return if c.precio.is_zero() {
            Estado::SinPrecio
        } else {
            Estado::Valorada
        };
    }
    let Ok(e) = p.explosion_de(&BTreeMap::from([(partida.to_owned(), Decimal::ONE)])) else {
        return Estado::SinPrecio;
    };
    let hay = |n: Naturaleza| e.por_naturaleza(n).any(|r| !r.importe.is_zero());
    if e.total.is_zero() {
        return Estado::SinPrecio;
    }
    let mo = hay(Naturaleza::ManoObra);
    let mat = hay(Naturaleza::Material);
    if hay(Naturaleza::Subcontrata) || mo == mat {
        // Subcontratada, completa, o sin ninguna de las dos (p. ej. solo
        // maquinaria u otros): no se marca
        Estado::Valorada
    } else if mo {
        Estado::SoloManoObra
    } else {
        Estado::SoloMaterial
    }
}

/// Estado de todas las partidas y capítulos de la obra; el de un capítulo
/// es el peor de lo que contiene.
pub fn estados(p: &Presupuesto) -> HashMap<String, Estado> {
    fn rec(p: &Presupuesto, cap: &str, out: &mut HashMap<String, Estado>) -> Estado {
        let mut peor = Estado::Valorada;
        let Ok(c) = p.concepto(cap) else { return peor };
        for l in &c.descomposicion {
            let Some(h) = p.conceptos.get(&l.hijo) else { continue };
            let e = if h.naturaleza == Naturaleza::Capitulo {
                rec(p, &l.hijo, out)
            } else if h.naturaleza.es_basico() {
                continue;
            } else {
                *out.entry(l.hijo.clone()).or_insert_with(|| estado(p, &l.hijo))
            };
            peor = peor.peor(e);
        }
        out.insert(cap.to_owned(), peor);
        peor
    }
    let mut out = HashMap::new();
    if p.validar().is_ok() {
        rec(p, &p.raiz, &mut out);
    }
    out
}
