// SPDX-License-Identifier: GPL-3.0-or-later
//! Propuestas de precio hechas por una IA (por MCP) que el usuario aprueba
//! o rechaza en el programa. Nada se aplica sin aprobación.
//!
//! Una propuesta es, para una partida, **un descompuesto** (líneas con
//! recursos existentes o nuevos) o **un precio directo**, con un grado de
//! confianza y la justificación (de dónde salen los precios y rendimientos).
//!
//! Al aceptar ([`Presupuesto::aplicar_propuesta`]):
//! - Los recursos que ya existen en la obra se reutilizan **con su precio**
//!   (si la IA proponía otro, se informa en `precios_distintos`).
//! - Los nuevos se dan de alta con la naturaleza, unidad y precio propuestos.
//! - El descompuesto de la partida se sustituye por el propuesto.
//!
//! Si algo no es válido, el presupuesto no cambia.

use crate::concepto::{Concepto, LineaDescomposicion, Naturaleza};
use crate::error::ErrorMotor;
use crate::presupuesto::Presupuesto;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Confianza {
    Baja,
    Media,
    Alta,
}

impl Confianza {
    pub fn nombre(self) -> &'static str {
        match self {
            Self::Alta => "alta",
            Self::Media => "media",
            Self::Baja => "baja",
        }
    }

    pub fn leer(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "alta" | "high" => Some(Self::Alta),
            "media" | "medium" => Some(Self::Media),
            "baja" | "low" => Some(Self::Baja),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineaPropuesta {
    pub codigo: String,
    /// Rendimiento (en una línea porcentual, los puntos).
    pub rendimiento: Decimal,
    /// Solo para recursos nuevos (si el código ya existe se ignoran).
    #[serde(default)]
    pub naturaleza: Option<Naturaleza>,
    #[serde(default)]
    pub unidad: String,
    #[serde(default)]
    pub resumen: String,
    #[serde(default)]
    pub precio: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Contenido {
    Descompuesto(Vec<LineaPropuesta>),
    PrecioDirecto(Decimal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstadoPropuesta {
    Pendiente,
    Aceptada,
    Rechazada,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Propuesta {
    pub id: u64,
    pub partida: String,
    pub contenido: Contenido,
    pub confianza: Confianza,
    pub justificacion: String,
    /// Quién propone («Claude», «otra IA»…).
    #[serde(default)]
    pub autor: String,
    pub estado: EstadoPropuesta,
}

/// Resultado de aplicar una propuesta.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Aplicada {
    pub precio_antes: Decimal,
    pub precio_despues: Decimal,
    /// Recursos dados de alta.
    pub nuevos: Vec<String>,
    /// (código, precio en la obra, precio propuesto) cuando no coinciden.
    pub precios_distintos: Vec<(String, Decimal, Decimal)>,
}

impl Presupuesto {
    /// Aplica la propuesta (ver el módulo). Si falla, no cambia nada.
    pub fn aplicar_propuesta(&mut self, prop: &Propuesta) -> Result<Aplicada, ErrorMotor> {
        let copia = self.clone();
        let r = self.aplicar_propuesta_sin_deshacer(prop);
        if r.is_err() {
            *self = copia;
        }
        r
    }

    /// Lo que pasaría al aceptar, sin cambiar el presupuesto.
    pub fn simular_propuesta(&self, prop: &Propuesta) -> Result<Aplicada, ErrorMotor> {
        self.clone().aplicar_propuesta_sin_deshacer(prop)
    }

    fn aplicar_propuesta_sin_deshacer(&mut self, prop: &Propuesta) -> Result<Aplicada, ErrorMotor> {
        let partida = prop.partida.as_str();
        let c = self.concepto(partida)?;
        if c.naturaleza != Naturaleza::Partida {
            return Err(ErrorMotor::ValorInvalido(format!("«{partida}» no es una partida")));
        }
        let mut out = Aplicada {
            precio_antes: self.precio(partida)?,
            ..Aplicada::default()
        };
        match &prop.contenido {
            Contenido::PrecioDirecto(precio) => {
                if !self.concepto(partida)?.descomposicion.is_empty() {
                    return Err(ErrorMotor::ValorInvalido(format!(
                        "«{partida}» tiene descompuesto: proponga un descompuesto, no un precio directo"
                    )));
                }
                self.fijar_precio(partida, *precio)?;
            }
            Contenido::Descompuesto(lineas) => {
                if lineas.is_empty() {
                    return Err(ErrorMotor::ValorInvalido("el descompuesto propuesto está vacío".into()));
                }
                let mut nuevas = Vec::new();
                for l in lineas {
                    if l.codigo == partida {
                        return Err(ErrorMotor::ValorInvalido(format!(
                            "«{partida}» no puede formar parte de sí misma"
                        )));
                    }
                    if l.rendimiento.is_zero() {
                        return Err(ErrorMotor::ValorInvalido(format!("«{}» con rendimiento 0", l.codigo)));
                    }
                    if nuevas.iter().any(|x: &LineaDescomposicion| x.hijo == l.codigo) {
                        return Err(ErrorMotor::ValorInvalido(format!("«{}» repetido", l.codigo)));
                    }
                    match self.conceptos.get(&l.codigo) {
                        Some(h) => {
                            if h.naturaleza == Naturaleza::Capitulo {
                                return Err(ErrorMotor::ValorInvalido(format!("«{}» es un capítulo", l.codigo)));
                            }
                            if let Some(pp) = l.precio
                                && h.naturaleza.es_basico()
                                && pp != h.precio
                            {
                                out.precios_distintos.push((l.codigo.clone(), h.precio, pp));
                            }
                        }
                        None => {
                            let n = l.naturaleza.ok_or_else(|| {
                                ErrorMotor::ValorInvalido(format!(
                                    "«{}» no existe en la obra: indique naturaleza, unidad, resumen y precio",
                                    l.codigo
                                ))
                            })?;
                            let nuevo = match n {
                                Naturaleza::Porcentaje => Concepto::porcentaje(&l.codigo, &l.resumen),
                                Naturaleza::Capitulo | Naturaleza::Partida => {
                                    return Err(ErrorMotor::ValorInvalido(format!(
                                        "«{}»: solo se pueden crear recursos básicos o porcentajes",
                                        l.codigo
                                    )));
                                }
                                n => {
                                    let precio = l.precio.ok_or_else(|| {
                                        ErrorMotor::ValorInvalido(format!("«{}» nuevo sin precio", l.codigo))
                                    })?;
                                    if precio < Decimal::ZERO {
                                        return Err(ErrorMotor::ValorInvalido(format!(
                                            "«{}» con precio negativo",
                                            l.codigo
                                        )));
                                    }
                                    if l.unidad.trim().is_empty() || l.resumen.trim().is_empty() {
                                        return Err(ErrorMotor::ValorInvalido(format!(
                                            "«{}» nuevo sin unidad o sin resumen",
                                            l.codigo
                                        )));
                                    }
                                    Concepto::basico(&l.codigo, l.unidad.trim(), l.resumen.trim(), n, precio)
                                }
                            };
                            crate::edicion::validar_codigo(&l.codigo)?;
                            self.insertar(nuevo)?;
                            out.nuevos.push(l.codigo.clone());
                        }
                    }
                    nuevas.push(LineaDescomposicion::new(l.codigo.clone(), l.rendimiento));
                }
                let c = self.concepto_mut(partida)?;
                c.descomposicion = nuevas;
                c.precio = Decimal::ZERO;
            }
        }
        self.validar()?;
        out.precio_despues = self.precio(partida)?;
        Ok(out)
    }
}
