// SPDX-License-Identifier: GPL-3.0-or-later
//! Subcontrata a ratio aplicada al presupuesto.
//!
//! Caso típico: el montaje de VRF (máquinas interiores y exteriores,
//! instalación frigorífica, cableado de control y saneamiento) se contrata a
//! **X € por máquina**. La subcontrata pone la mano de obra y parte de los
//! materiales (tubería, cable, PVC); la empresa compra máquinas, mandos,
//! cajas y juntas y aporta los medios auxiliares.
//!
//! Al aplicar un [`PaqueteRatio`]:
//! 1. **Medidor**: se suman las cantidades totales de las partidas que cuentan
//!    (p. ej. interiores + exteriores = 27 máquinas). Importe = ratio × medidor.
//! 2. En cada partida afectada se **quitan** los recursos que pone la
//!    subcontrata y se añade una línea con el recurso de subcontrata (unidad
//!    «máquina», precio = ratio) en el sitio de la primera línea quitada.
//! 3. El importe se **reparte** entre las partidas en proporción al coste
//!    propio que se les quita: el rendimiento de la línea es la fracción de
//!    máquinas que le toca por unidad de partida, y la suma de todas es
//!    exactamente el medidor.
//! 4. Las líneas porcentuales (medios auxiliares que aporta la empresa) se
//!    conservan y se calculan también sobre la parte subcontratada.
//!
//! Los casos sencillos son el mismo mecanismo: subcontratar una partida
//! entera o solo su mano de obra a un precio por su propia unidad es un
//! paquete de una partida que se mide a sí misma.

use crate::concepto::{Concepto, LineaDescomposicion, Naturaleza};
use crate::error::ErrorMotor;
use crate::presupuesto::Presupuesto;
use rust_decimal::Decimal;
use std::collections::BTreeMap;

/// Decimales del rendimiento de la línea de subcontrata (fracción de unidad
/// de contratación por unidad de partida).
const DEC_RENDIMIENTO: u32 = 6;

#[derive(Debug, Clone, PartialEq)]
pub struct PaqueteRatio {
    /// Código del recurso de subcontrata (p. ej. «SUB.VRF»).
    pub codigo: String,
    pub descripcion: String,
    /// Unidad de contratación («máquina», «m2», «ud»...).
    pub unidad: String,
    /// Precio por unidad de contratación (el ratio).
    pub precio: Decimal,
    /// Partidas cuya cantidad total cuenta para el medidor.
    pub medidor: Vec<String>,
    /// Partida → recursos de su descompuesto que pasan a la subcontrata.
    pub asignaciones: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RepartoPartida {
    pub partida: String,
    /// Cantidad total de la partida en la obra.
    pub cantidad: Decimal,
    /// Coste propio retirado (total en la obra).
    pub retirado: Decimal,
    /// Unidades de contratación por unidad de partida.
    pub rendimiento: Decimal,
    /// Importe de subcontrata que le corresponde (total en la obra).
    pub contratado: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InformeRatio {
    /// Unidades de contratación (p. ej. 27 máquinas).
    pub medidor: Decimal,
    /// Ratio × medidor.
    pub importe: Decimal,
    pub retirado: Decimal,
    pub coste_antes: Decimal,
    pub coste_despues: Decimal,
    /// Horas de mano de obra propia que pasan a la subcontrata, por recurso.
    pub horas_liberadas: BTreeMap<String, Decimal>,
    pub reparto: Vec<RepartoPartida>,
}

impl InformeRatio {
    /// Positivo = la subcontrata sale más barata que hacerlo con medios propios.
    pub fn ahorro(&self) -> Decimal {
        self.coste_antes - self.coste_despues
    }
}

impl Presupuesto {
    /// Aplica una subcontrata a ratio (ver el módulo). Si algo no es válido,
    /// el presupuesto no se modifica.
    pub fn aplicar_ratio(&mut self, paq: &PaqueteRatio) -> Result<InformeRatio, ErrorMotor> {
        let copia = self.clone();
        let r = self.aplicar_ratio_sin_deshacer(paq);
        if r.is_err() {
            *self = copia;
        }
        r
    }

    fn aplicar_ratio_sin_deshacer(&mut self, paq: &PaqueteRatio) -> Result<InformeRatio, ErrorMotor> {
        crate::edicion::validar_codigo(&paq.codigo)?;
        if paq.precio <= Decimal::ZERO {
            return Err(ErrorMotor::ValorInvalido("el ratio debe ser mayor que cero".into()));
        }
        if paq.asignaciones.is_empty() || paq.asignaciones.values().all(Vec::is_empty) {
            return Err(ErrorMotor::ValorInvalido(
                "elija al menos un recurso que pase a la subcontrata".into(),
            ));
        }
        match self.conceptos.get(&paq.codigo) {
            None => {}
            Some(c) if c.naturaleza == Naturaleza::Subcontrata => {
                return Err(ErrorMotor::ValorInvalido(format!(
                    "ya existe la subcontrata «{}»: use otro código",
                    paq.codigo
                )));
            }
            Some(_) => return Err(ErrorMotor::CodigoDuplicado(paq.codigo.clone())),
        }
        let cantidades = self.cantidades_partidas()?;
        let coste_antes = self.coste_directo()?;

        // 1. Medidor
        if paq.medidor.is_empty() {
            return Err(ErrorMotor::ValorInvalido(
                "elija las partidas que cuentan para el medidor".into(),
            ));
        }
        let mut medidor = Decimal::ZERO;
        for m in &paq.medidor {
            self.concepto(m)?;
            medidor += cantidades.get(m).copied().unwrap_or_default();
        }
        if medidor <= Decimal::ZERO {
            return Err(ErrorMotor::ValorInvalido(
                "el medidor es cero: las partidas no tienen cantidad".into(),
            ));
        }
        let importe = crate::redondear(paq.precio * medidor, self.decimales.importe);

        // 2. Coste propio retirado en cada partida
        let mut retirados = Vec::new();
        let mut horas = BTreeMap::new();
        for (partida, recursos) in &paq.asignaciones {
            if recursos.is_empty() {
                continue;
            }
            let c = self.concepto(partida)?;
            if c.naturaleza != Naturaleza::Partida || c.descomposicion.is_empty() {
                return Err(ErrorMotor::ValorInvalido(format!(
                    "«{partida}» no es una partida descompuesta"
                )));
            }
            let lineas = self.lineas_valoradas(partida)?;
            let mut unitario = Decimal::ZERO;
            for r in recursos {
                let l = lineas
                    .iter()
                    .find(|l| &l.hijo == r)
                    .ok_or_else(|| ErrorMotor::RecursoNoEnPartida {
                        partida: partida.clone(),
                        recurso: r.clone(),
                    })?;
                if l.naturaleza == Naturaleza::Porcentaje {
                    return Err(ErrorMotor::ValorInvalido(format!(
                        "«{r}» es un porcentaje (medios auxiliares): se queda en la partida"
                    )));
                }
                unitario += l.importe;
                if l.naturaleza == Naturaleza::ManoObra {
                    let q = cantidades.get(partida).copied().unwrap_or_default();
                    *horas.entry(r.clone()).or_insert(Decimal::ZERO) += l.cantidad * q;
                }
            }
            let cantidad = cantidades.get(partida).copied().unwrap_or_default();
            if cantidad.is_zero() {
                return Err(ErrorMotor::ValorInvalido(format!(
                    "«{partida}» no tiene cantidad en ningún capítulo"
                )));
            }
            retirados.push((partida.clone(), cantidad, unitario * cantidad));
        }
        let retirado: Decimal = retirados.iter().map(|r| r.2).sum();
        if retirado <= Decimal::ZERO {
            return Err(ErrorMotor::ValorInvalido(
                "los recursos elegidos no tienen coste: no hay nada que repartir".into(),
            ));
        }

        // 3. Recurso de subcontrata (el código ya se ha comprobado)
        self.insertar(Concepto::basico(
            paq.codigo.clone(),
            paq.unidad.clone(),
            paq.descripcion.clone(),
            Naturaleza::Subcontrata,
            paq.precio,
        ))?;

        // 4. Reparto: rendimientos a 6 decimales; la partida de menor cantidad
        // absorbe el redondeo para que Σ rendimiento × cantidad = medidor.
        let mut rend: Vec<Decimal> = retirados
            .iter()
            .map(|(_, cantidad, ret)| crate::redondear(medidor * *ret / retirado / *cantidad, DEC_RENDIMIENTO))
            .collect();
        if let Some(i) = (0..retirados.len()).min_by_key(|&i| retirados[i].1) {
            let resto: Decimal = (0..retirados.len())
                .filter(|&j| j != i)
                .map(|j| rend[j] * retirados[j].1)
                .sum();
            rend[i] = crate::redondear((medidor - resto) / retirados[i].1, DEC_RENDIMIENTO);
        }

        // 5. Quitar recursos y poner la línea de subcontrata
        let mut reparto = Vec::new();
        for ((partida, cantidad, ret), rendimiento) in retirados.into_iter().zip(rend) {
            let recursos = &paq.asignaciones[&partida];
            let d = &mut self.concepto_mut(&partida)?.descomposicion;
            let pos = d.iter().position(|l| recursos.contains(&l.hijo)).unwrap_or(d.len());
            d.retain(|l| !recursos.contains(&l.hijo));
            d.insert(
                pos.min(d.len()),
                LineaDescomposicion::new(paq.codigo.clone(), rendimiento),
            );
            reparto.push(RepartoPartida {
                contratado: crate::redondear(paq.precio * rendimiento * cantidad, 2),
                partida,
                cantidad,
                retirado: ret,
                rendimiento,
            });
        }
        self.validar()?;
        Ok(InformeRatio {
            medidor,
            importe,
            retirado: crate::redondear(retirado, 2),
            coste_antes,
            coste_despues: self.coste_directo()?,
            horas_liberadas: horas,
            reparto,
        })
    }
}
