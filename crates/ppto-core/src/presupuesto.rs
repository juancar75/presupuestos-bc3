// SPDX-License-Identifier: GPL-3.0-or-later
//! Presupuesto: árbol de conceptos, cálculo de precios e importes.
//!
//! Reglas de cálculo (ver `docs/reglas-calculo.md` §2 y §4):
//! - En un **descompuesto** (padre = partida o auxiliar): cantidad de línea =
//!   factor × rendimiento redondeado a `rendimiento`; importe de línea =
//!   cantidad × precio del hijo redondeado a `importe_linea`; precio de la
//!   partida = Σ importes redondeado a `precio`.
//! - Línea **porcentual**: cantidad en %, base = Σ importes de las líneas
//!   anteriores del mismo descompuesto, importe = cantidad × base / 100.
//! - En un **capítulo**: cantidad = medición total (si existe) o factor ×
//!   rendimiento, redondeada a `medicion`; importe = cantidad × precio de la
//!   partida redondeado a `importe`; total de capítulo = Σ importes.

use crate::concepto::{Concepto, LineaDescomposicion, Naturaleza};
use crate::decimales::{Decimales, redondear};
use crate::error::ErrorMotor;
use crate::medicion::Medicion;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Presupuesto {
    pub nombre: String,
    pub decimales: Decimales,
    /// Código del concepto raíz (capítulo «obra»).
    pub raiz: String,
    pub conceptos: BTreeMap<String, Concepto>,
    /// Mediciones por par (padre, hijo).
    pub mediciones: BTreeMap<(String, String), Medicion>,
}

/// Una línea de descomposición ya valorada.
#[derive(Debug, Clone, PartialEq)]
pub struct LineaValorada {
    pub hijo: String,
    pub naturaleza: Naturaleza,
    pub cantidad: Decimal,
    /// Precio del hijo; en líneas porcentuales, la base sobre la que se aplica.
    pub precio: Decimal,
    pub importe: Decimal,
}

impl Presupuesto {
    pub fn new(nombre: impl Into<String>, raiz: impl Into<String>, resumen: impl Into<String>) -> Self {
        let raiz = raiz.into();
        let mut conceptos = BTreeMap::new();
        conceptos.insert(raiz.clone(), Concepto::capitulo(raiz.clone(), resumen));
        Self {
            nombre: nombre.into(),
            decimales: Decimales::default(),
            raiz,
            conceptos,
            mediciones: BTreeMap::new(),
        }
    }

    pub fn insertar(&mut self, c: Concepto) -> Result<(), ErrorMotor> {
        if self.conceptos.contains_key(&c.codigo) {
            return Err(ErrorMotor::CodigoDuplicado(c.codigo));
        }
        self.conceptos.insert(c.codigo.clone(), c);
        Ok(())
    }

    pub fn concepto(&self, codigo: &str) -> Result<&Concepto, ErrorMotor> {
        self.conceptos
            .get(codigo)
            .ok_or_else(|| ErrorMotor::ConceptoInexistente(codigo.to_owned()))
    }

    fn concepto_mut(&mut self, codigo: &str) -> Result<&mut Concepto, ErrorMotor> {
        self.conceptos
            .get_mut(codigo)
            .ok_or_else(|| ErrorMotor::ConceptoInexistente(codigo.to_owned()))
    }

    /// Cuelga un capítulo o partida de un capítulo padre.
    pub fn colgar(&mut self, padre: &str, hijo: &str, medicion: Option<Medicion>) -> Result<(), ErrorMotor> {
        self.concepto(hijo)?;
        let rendimiento = match &medicion {
            Some(m) => {
                let unidad = self.concepto(hijo)?.unidad.clone();
                m.calcular(&self.decimales, &unidad)?.total
            }
            None => Decimal::ONE,
        };
        self.concepto_mut(padre)?
            .descomposicion
            .push(LineaDescomposicion::new(hijo, rendimiento));
        if let Some(m) = medicion {
            self.mediciones.insert((padre.to_owned(), hijo.to_owned()), m);
        }
        Ok(())
    }

    /// Comprueba que todos los hijos existen y que no hay ciclos.
    pub fn validar(&self) -> Result<(), ErrorMotor> {
        let mut estado: HashMap<&str, u8> = HashMap::new(); // 1 = en pila, 2 = cerrado
        let mut pila: Vec<String> = Vec::new();
        for codigo in self.conceptos.keys() {
            self.dfs(codigo, &mut estado, &mut pila)?;
        }
        Ok(())
    }

    fn dfs<'a>(
        &'a self,
        codigo: &'a str,
        estado: &mut HashMap<&'a str, u8>,
        pila: &mut Vec<String>,
    ) -> Result<(), ErrorMotor> {
        match estado.get(codigo) {
            Some(2) => return Ok(()),
            Some(1) => {
                let ini = pila.iter().position(|c| c == codigo).unwrap_or(0);
                let mut ciclo = pila[ini..].to_vec();
                ciclo.push(codigo.to_owned());
                return Err(ErrorMotor::ReferenciaCircular(ciclo));
            }
            _ => {}
        }
        let c = self.concepto(codigo)?;
        estado.insert(codigo, 1);
        pila.push(codigo.to_owned());
        for l in &c.descomposicion {
            self.dfs(&l.hijo, estado, pila)?;
        }
        pila.pop();
        estado.insert(codigo, 2);
        Ok(())
    }

    /// Precio unitario de un concepto (total si es capítulo).
    pub fn precio(&self, codigo: &str) -> Result<Decimal, ErrorMotor> {
        self.validar()?;
        let mut cache = HashMap::new();
        self.precio_rec(codigo, &mut cache)
    }

    /// Presupuesto de ejecución material (total del concepto raíz).
    pub fn pem(&self) -> Result<Decimal, ErrorMotor> {
        self.precio(&self.raiz)
    }

    /// Líneas valoradas de un concepto.
    pub fn lineas_valoradas(&self, codigo: &str) -> Result<Vec<LineaValorada>, ErrorMotor> {
        self.validar()?;
        let mut cache = HashMap::new();
        self.lineas_rec(codigo, &mut cache)
    }

    fn precio_rec(&self, codigo: &str, cache: &mut HashMap<String, Decimal>) -> Result<Decimal, ErrorMotor> {
        if let Some(p) = cache.get(codigo) {
            return Ok(*p);
        }
        let c = self.concepto(codigo)?;
        let d = &self.decimales;
        let p = match c.naturaleza {
            n if n.es_basico() => redondear(c.precio, d.precio),
            Naturaleza::Porcentaje => Decimal::ZERO,
            Naturaleza::Capitulo => {
                let s: Decimal = self.lineas_rec(codigo, cache)?.iter().map(|l| l.importe).sum();
                redondear(s, d.importe)
            }
            _ => {
                let s: Decimal = self.lineas_rec(codigo, cache)?.iter().map(|l| l.importe).sum();
                redondear(s, d.precio)
            }
        };
        cache.insert(codigo.to_owned(), p);
        Ok(p)
    }

    pub(crate) fn lineas_rec(
        &self,
        codigo: &str,
        cache: &mut HashMap<String, Decimal>,
    ) -> Result<Vec<LineaValorada>, ErrorMotor> {
        let c = self.concepto(codigo)?;
        let d = &self.decimales;
        let es_capitulo = c.naturaleza == Naturaleza::Capitulo;
        let mut out = Vec::with_capacity(c.descomposicion.len());
        let mut acumulado = Decimal::ZERO;
        for l in &c.descomposicion {
            let hijo = self.concepto(&l.hijo)?;
            let linea = if es_capitulo {
                let cantidad = redondear(l.cantidad(), d.medicion);
                let precio = self.precio_rec(&l.hijo, cache)?;
                LineaValorada {
                    hijo: l.hijo.clone(),
                    naturaleza: hijo.naturaleza,
                    cantidad,
                    precio,
                    importe: redondear(cantidad * precio, d.importe),
                }
            } else if hijo.naturaleza == Naturaleza::Porcentaje {
                let cantidad = redondear(l.cantidad(), d.rendimiento);
                LineaValorada {
                    hijo: l.hijo.clone(),
                    naturaleza: hijo.naturaleza,
                    cantidad,
                    precio: acumulado,
                    importe: redondear(cantidad * acumulado / Decimal::ONE_HUNDRED, d.importe_linea),
                }
            } else {
                let cantidad = redondear(l.cantidad(), d.rendimiento);
                let precio = self.precio_rec(&l.hijo, cache)?;
                LineaValorada {
                    hijo: l.hijo.clone(),
                    naturaleza: hijo.naturaleza,
                    cantidad,
                    precio,
                    importe: redondear(cantidad * precio, d.importe_linea),
                }
            };
            acumulado += linea.importe;
            out.push(linea);
        }
        Ok(out)
    }

    /// Cantidad total de cada partida en el presupuesto (suma de todas sus
    /// apariciones en capítulos, multiplicada por la cantidad de los capítulos
    /// que la contienen). No incluye precios auxiliares usados dentro de
    /// descompuestos; para eso está la explosión de recursos.
    pub fn cantidades_partidas(&self) -> Result<BTreeMap<String, Decimal>, ErrorMotor> {
        self.validar()?;
        let mut out = BTreeMap::new();
        self.cantidades_rec(&self.raiz, Decimal::ONE, &mut out)?;
        Ok(out)
    }

    fn cantidades_rec(
        &self,
        capitulo: &str,
        mult: Decimal,
        out: &mut BTreeMap<String, Decimal>,
    ) -> Result<(), ErrorMotor> {
        for l in &self.concepto(capitulo)?.descomposicion {
            let cant = mult * redondear(l.cantidad(), self.decimales.medicion);
            match self.concepto(&l.hijo)?.naturaleza {
                Naturaleza::Capitulo => self.cantidades_rec(&l.hijo, cant, out)?,
                _ => *out.entry(l.hijo.clone()).or_insert(Decimal::ZERO) += cant,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::concepto::LineaDescomposicion as L;
    use rust_decimal_macros::dec;

    #[test]
    fn detecta_referencia_circular() {
        let mut p = Presupuesto::new("t", "OBRA", "Obra");
        p.insertar(Concepto::partida("A", "ud", "A", vec![L::new("B", dec!(1))]))
            .unwrap();
        p.insertar(Concepto::partida("B", "ud", "B", vec![L::new("C", dec!(1))]))
            .unwrap();
        p.insertar(Concepto::partida("C", "ud", "C", vec![L::new("A", dec!(1))]))
            .unwrap();
        match p.precio("A") {
            Err(ErrorMotor::ReferenciaCircular(c)) => assert_eq!(c, vec!["A", "B", "C", "A"]),
            otro => panic!("se esperaba ciclo, se obtuvo {otro:?}"),
        }
    }

    #[test]
    fn detecta_autoreferencia_y_hijo_inexistente() {
        let mut p = Presupuesto::new("t", "OBRA", "Obra");
        p.insertar(Concepto::partida("A", "ud", "A", vec![L::new("A", dec!(1))]))
            .unwrap();
        assert!(matches!(p.validar(), Err(ErrorMotor::ReferenciaCircular(_))));
        let mut q = Presupuesto::new("t", "OBRA", "Obra");
        q.insertar(Concepto::partida("A", "ud", "A", vec![L::new("NOEXISTE", dec!(1))]))
            .unwrap();
        assert_eq!(q.validar(), Err(ErrorMotor::ConceptoInexistente("NOEXISTE".into())));
    }

    #[test]
    fn codigo_duplicado() {
        let mut p = Presupuesto::new("t", "OBRA", "Obra");
        assert_eq!(
            p.insertar(Concepto::capitulo("OBRA", "x")),
            Err(ErrorMotor::CodigoDuplicado("OBRA".into()))
        );
    }

    #[test]
    fn precio_auxiliar_jerarquico_reutilizable() {
        // Mortero M-5 como precio auxiliar (m³) usado por dos partidas.
        let mut p = Presupuesto::new("t", "OBRA", "Obra");
        let b = |c: &str, u: &str, n, pr| Concepto::basico(c, u, c, n, pr);
        p.insertar(b("CEM", "t", Naturaleza::Material, dec!(110.00))).unwrap();
        p.insertar(b("ARE", "m3", Naturaleza::Material, dec!(18.50))).unwrap();
        p.insertar(b("PEON", "h", Naturaleza::ManoObra, dec!(19.80))).unwrap();
        p.insertar(b("LAD", "ud", Naturaleza::Material, dec!(0.21))).unwrap();
        p.insertar(Concepto::partida(
            "MORT",
            "m3",
            "Mortero M-5",
            vec![
                L::new("CEM", dec!(0.250)),
                L::new("ARE", dec!(1.100)),
                L::new("PEON", dec!(1.700)),
            ],
        ))
        .unwrap();
        // 27,50 + 20,35 + 33,66 = 81,51 €/m³
        assert_eq!(p.precio("MORT").unwrap(), dec!(81.51));
        p.insertar(Concepto::partida(
            "FAB",
            "m2",
            "Fábrica de ladrillo",
            vec![
                L::new("LAD", dec!(42)),
                L::new("MORT", dec!(0.020)),
                L::new("PEON", dec!(0.350)),
            ],
        ))
        .unwrap();
        // 8,82 + 1,63 + 6,93 = 17,38 €/m²
        assert_eq!(p.precio("FAB").unwrap(), dec!(17.38));
        let lv = p.lineas_valoradas("FAB").unwrap();
        assert_eq!(lv[1].importe, dec!(1.63));
    }
}
