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
    /// sobre la suma de las líneas anteriores del mismo descompuesto cuyo
    /// código empieza por la máscara (prefijo del código antes de `%`/`&`).
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
    /// Precio de tarifa del fabricante: el precio neto (`precio`) se calcula
    /// con el descuento del fabricante del presupuesto (ver `descuentos`).
    #[serde(default)]
    pub tarifa: Option<PrecioTarifa>,
    /// Margen propio de la partida, en % sobre su precio (venta = precio ×
    /// (1 + margen/100)). `None`: el general de la obra (GG + BI).
    #[serde(default)]
    pub margen: Option<Decimal>,
    /// Precio de venta bloqueado (sin IVA): la partida se vende a este
    /// importe aunque cambie su coste; prevalece sobre el margen.
    #[serde(default)]
    pub venta_fija: Option<Decimal>,
    /// De dónde sale el precio (oferta de fabricante o proveedor y su
    /// documento), para poder justificarlo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origen: Option<OrigenPrecio>,
}

/// Procedencia del precio de un recurso: la oferta que lo justifica.
///
/// El documento se guarda con la **ruta que se elija en cada caso** (carpeta
/// de la obra, unidad compartida, OneDrive…); no se copia.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OrigenPrecio {
    /// Fabricante o proveedor que oferta.
    pub fabricante: String,
    /// Número o referencia de la oferta.
    pub oferta: String,
    /// Fecha de la oferta (AAAA-MM-DD).
    #[serde(default)]
    pub fecha: String,
    /// Fin de validez (AAAA-MM-DD), si la oferta la indica.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validez: Option<String>,
    /// Referencia del artículo en la oferta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referencia: Option<String>,
    /// Precio neto ofertado (para avisar si el del recurso ya no coincide).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub neto: Option<Decimal>,
    /// Ruta del documento de la oferta (PDF, correo…), tal como se eligió.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documento: Option<String>,
}

impl OrigenPrecio {
    /// La oferta ha caducado a fecha `hoy` (AAAA-MM-DD).
    pub fn vencida(&self, hoy: &str) -> bool {
        self.validez.as_deref().is_some_and(|v| !v.is_empty() && v < hoy)
    }

    /// El precio del recurso ya no es el ofertado.
    pub fn precio_distinto(&self, precio: Decimal) -> bool {
        self.neto.is_some_and(|n| n != precio)
    }

    /// Texto corto: «KOOLAIR · oferta 2026-118 (2026-05-04)».
    pub fn etiqueta(&self) -> String {
        let mut t = self.fabricante.clone();
        if !self.oferta.is_empty() {
            t.push_str(&format!(" · oferta {}", self.oferta));
        }
        if !self.fecha.is_empty() {
            t.push_str(&format!(" ({})", self.fecha));
        }
        t
    }
}

/// PVP de tarifa y fabricante de un recurso.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrecioTarifa {
    pub fabricante: String,
    /// Grupo de producto del fabricante (p. ej. «E08» de S&P), si tiene
    /// descuento propio.
    #[serde(default)]
    pub grupo: Option<String>,
    pub pvp: Decimal,
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
            tarifa: None,
            margen: None,
            venta_fija: None,
            origen: None,
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
            tarifa: None,
            margen: None,
            venta_fija: None,
            origen: None,
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
            tarifa: None,
            margen: None,
            venta_fija: None,
            origen: None,
        }
    }

    pub fn porcentaje(codigo: impl Into<String>, resumen: impl Into<String>) -> Self {
        Self::basico(codigo, "%", resumen, Naturaleza::Porcentaje, Decimal::ZERO)
    }
}
