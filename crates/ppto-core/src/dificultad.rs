// SPDX-License-Identifier: GPL-3.0-or-later
//! Coeficiente de dificultad: más horas de mano de obra **propia** en una
//! obra (o en un capítulo o una partida) difícil, sin tocar el precio de la
//! hora, los materiales ni las subcontratas.
//!
//! Se multiplica el rendimiento de cada línea de mano de obra (naturaleza
//! [`Naturaleza::ManoObra`]) de las partidas del ámbito y de sus precios
//! auxiliares, redondeado a los decimales de rendimiento de la obra. Los
//! porcentajes (medios auxiliares) se recalculan solos.
//!
//! Una partida o un auxiliar que también se usa **fuera** del ámbito no se
//! toca (cambiaría el precio en otros capítulos): se informa en `omitidas`.

use crate::concepto::Naturaleza;
use crate::error::ErrorMotor;
use crate::presupuesto::Presupuesto;
use crate::redondear;
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq)]
pub struct InformeDificultad {
    pub coeficiente: Decimal,
    /// Horas por recurso de mano de obra (toda la obra) antes y después.
    pub horas_antes: BTreeMap<String, Decimal>,
    pub horas_despues: BTreeMap<String, Decimal>,
    pub coste_antes: Decimal,
    pub coste_despues: Decimal,
    /// Conceptos (partidas y auxiliares) modificados.
    pub modificadas: Vec<String>,
    /// Partidas o auxiliares del ámbito compartidos con el resto de la obra.
    pub omitidas: Vec<String>,
}

impl InformeDificultad {
    pub fn total_antes(&self) -> Decimal {
        self.horas_antes.values().copied().sum()
    }
    pub fn total_despues(&self) -> Decimal {
        self.horas_despues.values().copied().sum()
    }
}

impl Presupuesto {
    fn horas_mano_obra(&self) -> Result<BTreeMap<String, Decimal>, ErrorMotor> {
        Ok(self
            .explosion_recursos()?
            .por_naturaleza(Naturaleza::ManoObra)
            .map(|r| (r.codigo.clone(), r.cantidad))
            .collect())
    }

    /// Partidas colgadas de `codigo` (capítulo o partida). Si `excluir` es un
    /// capítulo, no se entra en él.
    fn partidas_bajo(&self, codigo: &str, excluir: Option<&str>, out: &mut BTreeSet<String>) {
        if Some(codigo) == excluir {
            return;
        }
        let Some(c) = self.conceptos.get(codigo) else {
            return;
        };
        match c.naturaleza {
            Naturaleza::Partida => {
                out.insert(codigo.to_owned());
            }
            Naturaleza::Capitulo => {
                for l in &c.descomposicion {
                    self.partidas_bajo(&l.hijo, excluir, out);
                }
            }
            _ => {}
        }
    }

    /// Partidas y auxiliares descompuestos que cuelgan de `raices`.
    fn cierre(&self, raices: &BTreeSet<String>) -> BTreeSet<String> {
        let mut vistos = BTreeSet::new();
        let mut pila: Vec<String> = raices.iter().cloned().collect();
        while let Some(c) = pila.pop() {
            if !vistos.insert(c.clone()) {
                continue;
            }
            if let Some(x) = self.conceptos.get(&c) {
                for l in &x.descomposicion {
                    if self
                        .conceptos
                        .get(&l.hijo)
                        .is_some_and(|h| h.naturaleza == Naturaleza::Partida)
                    {
                        pila.push(l.hijo.clone());
                    }
                }
            }
        }
        vistos
    }

    /// Multiplica por `coeficiente` las horas de mano de obra propia de
    /// `ambito` (la raíz = toda la obra, un capítulo o una partida). Si algo
    /// falla, el presupuesto no se modifica.
    pub fn aplicar_dificultad(&mut self, ambito: &str, coeficiente: Decimal) -> Result<InformeDificultad, ErrorMotor> {
        if coeficiente <= Decimal::ZERO {
            return Err(ErrorMotor::ValorInvalido(
                "el coeficiente debe ser mayor que cero".into(),
            ));
        }
        if coeficiente == Decimal::ONE {
            return Err(ErrorMotor::ValorInvalido("un coeficiente 1 no cambia nada".into()));
        }
        let a = self.concepto(ambito)?;
        if !matches!(a.naturaleza, Naturaleza::Capitulo | Naturaleza::Partida) {
            return Err(ErrorMotor::ValorInvalido(format!(
                "«{ambito}» no es un capítulo ni una partida"
            )));
        }
        let es_partida = a.naturaleza == Naturaleza::Partida;
        let horas_antes = self.horas_mano_obra()?;
        let coste_antes = self.coste_directo()?;

        let mut dentro = BTreeSet::new();
        self.partidas_bajo(ambito, None, &mut dentro);
        let mut fuera = BTreeSet::new();
        let raiz = self.raiz.clone();
        self.partidas_bajo(&raiz, if es_partida { None } else { Some(ambito) }, &mut fuera);
        if es_partida {
            fuera.remove(ambito);
        }
        let compartidos = self.cierre(&fuera);
        let mut omitidas = Vec::new();
        let mut modificables = Vec::new();
        for c in self.cierre(&dentro) {
            if compartidos.contains(&c) {
                omitidas.push(c);
            } else {
                modificables.push(c);
            }
        }

        let copia = self.clone();
        let dec = self.decimales.rendimiento;
        let mut modificadas = Vec::new();
        for c in &modificables {
            let mo: BTreeSet<String> = self.conceptos[c]
                .descomposicion
                .iter()
                .filter(|l| {
                    self.conceptos
                        .get(&l.hijo)
                        .is_some_and(|h| h.naturaleza == Naturaleza::ManoObra)
                })
                .map(|l| l.hijo.clone())
                .collect();
            if mo.is_empty() {
                continue;
            }
            for l in &mut self.concepto_mut(c)?.descomposicion {
                if mo.contains(&l.hijo) {
                    l.rendimiento = redondear(l.rendimiento * coeficiente, dec);
                }
            }
            modificadas.push(c.clone());
        }
        if modificadas.is_empty() {
            *self = copia;
            return Err(ErrorMotor::ValorInvalido(if omitidas.is_empty() {
                format!("«{ambito}» no tiene mano de obra propia")
            } else {
                format!("«{ambito}» solo tiene partidas compartidas con otros capítulos")
            }));
        }
        let r = (|| {
            self.validar()?;
            Ok(InformeDificultad {
                coeficiente,
                horas_antes,
                horas_despues: self.horas_mano_obra()?,
                coste_antes,
                coste_despues: self.coste_directo()?,
                modificadas,
                omitidas,
            })
        })();
        if r.is_err() {
            *self = copia;
        }
        r
    }
}
