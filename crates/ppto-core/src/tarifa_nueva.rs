// SPDX-License-Identifier: GPL-3.0-or-later
//! Pasar los recursos de un fabricante a su **tarifa nueva**.
//!
//! Cada año el fabricante publica tarifa: suben los PVP y los descuentos
//! por familia se mantienen. Un recurso con tarifa guarda la referencia del
//! artículo ([`PrecioTarifa::referencia`]; si no la tiene, se usa la del
//! origen del precio). [`Presupuesto::actualizar_a_tarifa`] busca cada
//! referencia en la tarifa nueva, cambia el PVP y recalcula el neto con el
//! descuento de su familia (simple o encadenado), que no se toca.
//!
//! El informe dice qué recursos cambian, cuáles no tienen referencia y
//! cuáles no aparecen en la tarifa nueva (artículo descatalogado o
//! referencia cambiada), que se quedan con el PVP anterior.

use crate::concepto::{Concepto, PrecioTarifa};
use crate::descuentos::normalizar;
use crate::error::ErrorMotor;
use crate::presupuesto::Presupuesto;
use rust_decimal::Decimal;
use std::collections::BTreeMap;

/// Cambio de PVP de un recurso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CambioPvp {
    pub codigo: String,
    pub referencia: String,
    pub pvp_anterior: Decimal,
    pub pvp_nuevo: Decimal,
    pub neto_anterior: Decimal,
    pub neto_nuevo: Decimal,
}

impl CambioPvp {
    /// Variación del PVP en % (2 decimales).
    pub fn variacion(&self) -> Option<Decimal> {
        (!self.pvp_anterior.is_zero())
            .then(|| ((self.pvp_nuevo - self.pvp_anterior) / self.pvp_anterior * Decimal::ONE_HUNDRED).round_dp(2))
    }
}

/// Resultado de pasar un fabricante a la tarifa nueva.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InformeTarifaNueva {
    /// Recursos con PVP nuevo distinto.
    pub cambiados: Vec<CambioPvp>,
    /// Recursos cuya referencia está en la tarifa nueva con el mismo PVP.
    pub iguales: usize,
    /// Recursos del fabricante sin referencia de artículo.
    pub sin_referencia: Vec<String>,
    /// (código, referencia) que no aparecen en la tarifa nueva.
    pub no_encontrados: Vec<(String, String)>,
}

impl InformeTarifaNueva {
    /// Subida media del PVP de los recursos cambiados e iguales, en %.
    pub fn media(&self) -> Option<Decimal> {
        let n = self.cambiados.len() + self.iguales;
        if n == 0 {
            return None;
        }
        let suma: Decimal = self.cambiados.iter().filter_map(CambioPvp::variacion).sum();
        Some((suma / Decimal::from(n as u64)).round_dp(2))
    }

    /// Resumen de una línea para la interfaz.
    pub fn resumen(&self, fabricante: &str, etiqueta: &str) -> String {
        let mut t = format!(
            "{} a tarifa {etiqueta}: {} recursos con PVP nuevo, {} sin cambio",
            normalizar(fabricante),
            self.cambiados.len(),
            self.iguales
        );
        if let Some(m) = self.media() {
            t.push_str(&format!(", subida media {} %", m.to_string().replace('.', ",")));
        }
        t.push('.');
        if !self.no_encontrados.is_empty() {
            t.push_str(&format!(
                " {} referencias no están en la tarifa nueva (se quedan con el PVP anterior): {}.",
                self.no_encontrados.len(),
                lista(self.no_encontrados.iter().map(|(c, r)| format!("{c} ({r})")))
            ));
        }
        if !self.sin_referencia.is_empty() {
            t.push_str(&format!(
                " {} recursos sin referencia de artículo: {}.",
                self.sin_referencia.len(),
                lista(self.sin_referencia.iter().cloned())
            ));
        }
        t.push_str(" Los descuentos por familia no cambian.");
        t
    }
}

fn lista(v: impl Iterator<Item = String>) -> String {
    let v: Vec<String> = v.collect();
    if v.len() > 8 {
        format!("{} y {} más", v[..8].join(", "), v.len() - 8)
    } else {
        v.join(", ")
    }
}

/// Referencia normalizada para comparar (sin espacios ni mayúsculas).
pub fn clave_referencia(r: &str) -> String {
    r.split_whitespace().collect::<String>().to_uppercase()
}

impl Presupuesto {
    /// Referencia de artículo de un recurso con tarifa: la suya o, si no la
    /// tiene, la del origen del precio.
    pub fn referencia_tarifa(c: &Concepto) -> Option<String> {
        let t = c.tarifa.as_ref()?;
        t.referencia
            .clone()
            .or_else(|| c.origen.as_ref().and_then(|o| o.referencia.clone()))
            .map(|r| r.trim().to_owned())
            .filter(|r| !r.is_empty() && r != "—" && r != "-")
    }

    /// Fija (o quita, con `None` o vacío) la referencia de artículo de un
    /// recurso con tarifa.
    pub fn fijar_referencia_tarifa(&mut self, codigo: &str, referencia: Option<&str>) -> Result<(), ErrorMotor> {
        let c = self.concepto_mut(codigo)?;
        let t = c
            .tarifa
            .as_mut()
            .ok_or_else(|| ErrorMotor::ValorInvalido(format!("«{codigo}» no tiene tarifa")))?;
        t.referencia = referencia.map(str::trim).filter(|r| !r.is_empty()).map(str::to_owned);
        Ok(())
    }

    /// Pasa los recursos de `fabricante` a los PVP de su tarifa nueva
    /// (`pvps`: referencia → PVP). Mantiene grupo y descuento de cada
    /// recurso y recalcula su neto. `etiqueta` («2027») se anota en el
    /// origen de los precios que venían de tarifa.
    pub fn actualizar_a_tarifa(
        &mut self,
        fabricante: &str,
        etiqueta: &str,
        pvps: &BTreeMap<String, Decimal>,
    ) -> InformeTarifaNueva {
        let fab = normalizar(fabricante);
        let precios: BTreeMap<String, Decimal> = pvps
            .iter()
            .filter(|(_, p)| **p >= Decimal::ZERO)
            .map(|(r, p)| (clave_referencia(r), *p))
            .collect();
        let mut inf = InformeTarifaNueva::default();
        let codigos: Vec<String> = self
            .conceptos
            .values()
            .filter(|c| c.tarifa.as_ref().is_some_and(|t| t.fabricante == fab))
            .map(|c| c.codigo.clone())
            .collect();
        for codigo in codigos {
            let c = &self.conceptos[&codigo];
            let Some(referencia) = Self::referencia_tarifa(c) else {
                inf.sin_referencia.push(codigo);
                continue;
            };
            let Some(&pvp) = precios.get(&clave_referencia(&referencia)) else {
                inf.no_encontrados.push((codigo, referencia));
                continue;
            };
            let t = c.tarifa.clone().expect("filtrado por tarifa");
            let nueva = PrecioTarifa {
                pvp,
                referencia: Some(referencia.clone()),
                ..t.clone()
            };
            let neto = self.neto(pvp, self.descuento_tarifa(&nueva));
            let neto_anterior = c.precio;
            let c = self.conceptos.get_mut(&codigo).expect("existe");
            c.tarifa = Some(nueva);
            c.precio = neto;
            if let Some(o) = c.origen.as_mut()
                && o.oferta.to_lowercase().starts_with("tarifa")
            {
                o.oferta = format!("Tarifa {} {etiqueta}", t.fabricante);
                o.neto = Some(neto);
            }
            if pvp == t.pvp {
                inf.iguales += 1;
            } else {
                inf.cambiados.push(CambioPvp {
                    codigo,
                    referencia,
                    pvp_anterior: t.pvp,
                    pvp_nuevo: pvp,
                    neto_anterior,
                    neto_nuevo: neto,
                });
            }
        }
        inf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::concepto::{Naturaleza, OrigenPrecio};
    use rust_decimal_macros::dec;

    fn obra() -> Presupuesto {
        let mut p = Presupuesto::new("t", "OBRA", "Obra");
        for (c, g, pvp, r) in [
            ("MT.A", "F1", dec!(100), Some("R-1")),
            ("MT.B", "F2", dec!(200), Some("R-2")),
            ("MT.C", "F1", dec!(50), None),
            ("MT.D", "F1", dec!(80), Some("R-9")),
        ] {
            p.conceptos.insert(
                c.into(),
                Concepto::basico(c, "ud", c, Naturaleza::Material, Decimal::ZERO),
            );
            p.fijar_tarifa_grupo(c, "Acme", Some(g), pvp).unwrap();
            p.fijar_referencia_tarifa(c, r).unwrap();
        }
        p.fijar_descuento("Acme/F1", dec!(60)).unwrap();
        p.fijar_descuento_texto("Acme/F2", "40+8").unwrap();
        p
    }

    #[test]
    fn sube_pvp_y_mantiene_descuento_por_familia() {
        let mut p = obra();
        assert_eq!(p.conceptos["MT.A"].precio, dec!(40.00));
        assert_eq!(p.conceptos["MT.B"].precio, dec!(110.40));
        let t = BTreeMap::from([("r-1".to_owned(), dec!(110)), ("R-2".into(), dec!(200))]);
        let inf = p.actualizar_a_tarifa("acme", "2027", &t);
        // 110 − 60 % y el encadenado 40+8 se conservan
        assert_eq!(p.conceptos["MT.A"].precio, dec!(44.00));
        assert_eq!(
            p.conceptos["MT.A"].tarifa.as_ref().unwrap().grupo.as_deref(),
            Some("F1")
        );
        assert_eq!(p.texto_descuento("Acme/F2"), "40+8");
        assert_eq!(p.conceptos["MT.B"].precio, dec!(110.40));
        assert_eq!(inf.cambiados.len(), 1);
        assert_eq!(inf.cambiados[0].variacion(), Some(dec!(10)));
        assert_eq!(inf.iguales, 1);
        assert_eq!(inf.sin_referencia, vec!["MT.C".to_owned()]);
        assert_eq!(inf.no_encontrados, vec![("MT.D".to_owned(), "R-9".to_owned())]);
        // Lo que no está en la tarifa nueva se queda como estaba
        assert_eq!(p.conceptos["MT.D"].precio, dec!(32.00));
        assert_eq!(inf.media(), Some(dec!(5)));
        assert!(inf.resumen("Acme", "2027").contains("1 recursos con PVP nuevo"));
    }

    #[test]
    fn usa_la_referencia_del_origen_y_anota_la_tarifa() {
        let mut p = obra();
        p.fijar_referencia_tarifa("MT.C", None).unwrap();
        p.conceptos.get_mut("MT.C").unwrap().origen = Some(OrigenPrecio {
            fabricante: "Acme".into(),
            oferta: "Tarifa Acme 2026".into(),
            referencia: Some("R-3".into()),
            ..Default::default()
        });
        let inf = p.actualizar_a_tarifa("ACME", "2027", &BTreeMap::from([("R-3".to_owned(), dec!(60))]));
        let c = &p.conceptos["MT.C"];
        assert_eq!(c.precio, dec!(24.00));
        assert_eq!(c.tarifa.as_ref().unwrap().referencia.as_deref(), Some("R-3"));
        let o = c.origen.as_ref().unwrap();
        assert_eq!((o.oferta.as_str(), o.neto), ("Tarifa ACME 2027", Some(dec!(24.00))));
        assert_eq!(inf.cambiados.len(), 1);
    }

    #[test]
    fn la_referencia_se_conserva_al_cambiar_grupo_o_pvp() {
        let mut p = obra();
        p.fijar_grupo("MT.A", "F2").unwrap();
        p.fijar_tarifa_grupo("MT.A", "Acme", Some("F2"), dec!(120)).unwrap();
        assert_eq!(
            p.conceptos["MT.A"].tarifa.as_ref().unwrap().referencia.as_deref(),
            Some("R-1")
        );
        assert!(p.fijar_referencia_tarifa("NO", Some("x")).is_err());
    }

    #[test]
    fn otro_fabricante_no_se_toca() {
        let mut p = obra();
        let inf = p.actualizar_a_tarifa("Otro", "2027", &BTreeMap::from([("R-1".to_owned(), dec!(999))]));
        assert_eq!(inf, InformeTarifaNueva::default());
        assert_eq!(p.conceptos["MT.A"].precio, dec!(40.00));
    }
}
