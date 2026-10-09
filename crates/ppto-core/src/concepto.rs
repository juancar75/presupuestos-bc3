// SPDX-License-Identifier: GPL-3.0-or-later
//! Conceptos y líneas de descomposición (equivalentes a los registros `~C` y `~D`).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Naturaleza económica del concepto.
///
/// FIEBDC-3 codifica en `~C` el tipo 0 (sin clasificar), 1 (mano de obra),
/// 2 (maquinaria y medios auxiliares) y 3 (materiales); los capítulos se
/// marcan con `#` en el código. «Subcontrata» y «Porcentaje» no tienen tipo
/// propio en todas las versiones del formato: su correspondencia se define
/// en P-004 y se valida con Presto 8.8 en P-012.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Naturaleza {
    Capitulo,
    /// Unidad de obra o precio auxiliar: su precio sale de su descomposición.
    Partida,
    ManoObra,
    Maquinaria,
    Material,
    Subcontrata,
    /// Línea porcentual (medios auxiliares, costes indirectos dentro del
    /// descompuesto…). Su cantidad es en puntos porcentuales y se aplica
    /// sobre la suma de las líneas anteriores del mismo descompuesto.
    Porcentaje,
    /// Tipo 0 de FIEBDC: otros costes con precio propio.
    Otros,
}

impl Naturaleza {
    /// Recursos básicos: tienen precio propio y no se descomponen.
    pub fn es_basico(self) -> bool {
        matches!(
            self,
            Self::ManoObra | Self::Maquinaria | Self::Material | Self::Subcontrata | Self::Otros
        )
    }
}

/// Línea de descomposición: un hijo dentro de un concepto padre.
///
/// Igual que en `~D`, la cantidad efectiva es `factor × rendimiento`.
/// En un capítulo, el rendimiento de cada partida es su medición total.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineaDescomposicion {
    pub hijo: String,
    pub factor: Decimal,
    pub rendimiento: Decimal,
}

impl LineaDescomposicion {
    pub fn new(hijo: impl Into<String>, rendimiento: Decimal) -> Self {
        Self {
            hijo: hijo.into(),
            factor: Decimal::ONE,
            rendimiento,
        }
    }

    pub fn cantidad(&self) -> Decimal {
        self.factor * self.rendimiento
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Concepto {
    pub codigo: String,
    pub unidad: String,
    pub resumen: String,
    pub naturaleza: Naturaleza,
    /// Precio propio. Solo se usa en recursos básicos; en partidas y capítulos
    /// el precio se calcula a partir de la descomposición.
    pub precio: Decimal,
    pub descomposicion: Vec<LineaDescomposicion>,
    pub texto: Option<String>,
}

impl Concepto {
    pub fn basico(
        codigo: impl Into<String>,
        unidad: impl Into<String>,
        resumen: impl Into<String>,
        naturaleza: Naturaleza,
        precio: Decimal,
    ) -> Self {
        Self {
            codigo: codigo.into(),
            unidad: unidad.into(),
            resumen: resumen.into(),
            naturaleza,
            precio,
            descomposicion: Vec::new(),
            texto: None,
        }
    }

    pub fn partida(
        codigo: impl Into<String>,
        unidad: impl Into<String>,
        resumen: impl Into<String>,
        lineas: Vec<LineaDescomposicion>,
    ) -> Self {
        Self {
            codigo: codigo.into(),
            unidad: unidad.into(),
            resumen: resumen.into(),
            naturaleza: Naturaleza::Partida,
            precio: Decimal::ZERO,
            descomposicion: lineas,
            texto: None,
        }
    }

    pub fn capitulo(codigo: impl Into<String>, resumen: impl Into<String>) -> Self {
        Self {
            codigo: codigo.into(),
            unidad: String::new(),
            resumen: resumen.into(),
            naturaleza: Naturaleza::Capitulo,
            precio: Decimal::ZERO,
            descomposicion: Vec::new(),
            texto: None,
        }
    }

    pub fn porcentaje(codigo: impl Into<String>, resumen: impl Into<String>) -> Self {
        Self::basico(codigo, "%", resumen, Naturaleza::Porcentaje, Decimal::ZERO)
    }
}
