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

/// Máscara de un concepto porcentual (FIEBDC-3): el prefijo del código antes
/// del primer «%» o «&». El porcentaje se aplica solo a las líneas anteriores
/// cuyo código empieza por ella; si está vacía, a todas.
pub fn mascara_porcentaje(codigo: &str) -> &str {
    codigo.find(['%', '&']).map_or("", |i| &codigo[..i])
}

/// Resultado de valorar el presupuesto completo.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Valoracion {
    /// Precio unitario de cada concepto (total, si es capítulo).
    pub precios: HashMap<String, Decimal>,
    /// Líneas valoradas de cada concepto con descomposición.
    pub lineas: HashMap<String, Vec<LineaValorada>>,
    pub pem: Decimal,
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

    /// Sustituye la medición de `hijo` en el capítulo `padre` y actualiza su
    /// cantidad (rendimiento de la línea) con el nuevo total. Devuelve los
    /// avisos de coherencia de unidades.
    pub fn actualizar_medicion(
        &mut self,
        padre: &str,
        hijo: &str,
        medicion: Medicion,
    ) -> Result<Vec<String>, ErrorMotor> {
        let unidad = self.concepto(hijo)?.unidad.clone();
        let r = medicion.calcular(&self.decimales, &unidad)?;
        let linea = self
            .concepto_mut(padre)?
            .descomposicion
            .iter_mut()
            .find(|l| l.hijo == hijo)
            .ok_or_else(|| ErrorMotor::PartidaSinMedicion(hijo.to_owned()))?;
        linea.factor = Decimal::ONE;
        linea.rendimiento = r.total;
        self.mediciones.insert((padre.to_owned(), hijo.to_owned()), medicion);
        Ok(r.avisos)
    }

    /// Cambia el rendimiento de la línea `hijo` dentro del descompuesto de `padre`.
    pub fn fijar_rendimiento(&mut self, padre: &str, hijo: &str, rendimiento: Decimal) -> Result<(), ErrorMotor> {
        let linea = self
            .concepto_mut(padre)?
            .descomposicion
            .iter_mut()
            .find(|l| l.hijo == hijo)
            .ok_or_else(|| ErrorMotor::RecursoNoEnPartida {
                partida: padre.to_owned(),
                recurso: hijo.to_owned(),
            })?;
        linea.factor = Decimal::ONE;
        linea.rendimiento = rendimiento;
        Ok(())
    }

    /// Cambia el precio propio de un recurso básico.
    pub fn fijar_precio(&mut self, codigo: &str, precio: Decimal) -> Result<(), ErrorMotor> {
        let c = self.concepto_mut(codigo)?;
        if !c.naturaleza.es_basico() {
            return Err(ErrorMotor::ValorInvalido(format!(
                "«{codigo}» no es un recurso básico: su precio sale de su descomposición"
            )));
        }
        if precio < Decimal::ZERO {
            return Err(ErrorMotor::ValorInvalido("el precio no puede ser negativo".into()));
        }
        c.precio = precio;
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

    /// Valora todo el presupuesto de una vez (una sola validación y una caché
    /// compartida). Es la forma eficiente de obtener precios y líneas de
    /// presupuestos grandes; `precio` y `lineas_valoradas` valoran uno a uno.
    pub fn valorar(&self) -> Result<Valoracion, ErrorMotor> {
        self.validar()?;
        let mut cache = HashMap::with_capacity(self.conceptos.len());
        let mut lineas = HashMap::new();
        for (codigo, c) in &self.conceptos {
            self.precio_rec(codigo, &mut cache)?;
            if !c.descomposicion.is_empty() {
                lineas.insert(codigo.clone(), self.lineas_rec(codigo, &mut cache)?);
            }
        }
        let pem = cache.get(&self.raiz).copied().unwrap_or_default();
        Ok(Valoracion {
            precios: cache,
            lineas,
            pem,
        })
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
        let mut out: Vec<LineaValorada> = Vec::with_capacity(c.descomposicion.len());
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
                // Base: líneas anteriores cuyo código empieza por la máscara
                // (parte del código antes de «%» o «&»; vacía = todas).
                let mascara = mascara_porcentaje(&l.hijo);
                let base: Decimal = out
                    .iter()
                    .filter(|p| p.hijo.starts_with(mascara))
                    .map(|p| p.importe)
                    .sum();
                let cantidad = redondear(l.cantidad(), d.rendimiento);
                LineaValorada {
                    hijo: l.hijo.clone(),
                    naturaleza: hijo.naturaleza,
                    cantidad,
                    precio: base,
                    importe: redondear(cantidad * base / Decimal::ONE_HUNDRED, d.importe_linea),
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
    fn porcentaje_con_mascara_solo_afecta_a_su_prefijo() {
        // O1 (mano de obra) 20,00 + M1 (material) 100,00; «O%MA» 10 % solo sobre «O…» = 2,00
        let mut p = Presupuesto::new("t", "OBRA", "Obra");
        p.insertar(Concepto::basico("O1", "h", "o", Naturaleza::ManoObra, dec!(20)))
            .unwrap();
        p.insertar(Concepto::basico("M1", "ud", "m", Naturaleza::Material, dec!(100)))
            .unwrap();
        p.insertar(Concepto::porcentaje("O%MA", "MA sobre mano de obra"))
            .unwrap();
        p.insertar(Concepto::porcentaje("%CI", "CI sobre todo")).unwrap();
        p.insertar(Concepto::partida(
            "P",
            "ud",
            "p",
            vec![
                L::new("O1", dec!(1)),
                L::new("M1", dec!(1)),
                L::new("O%MA", dec!(10)),
                L::new("%CI", dec!(3)),
            ],
        ))
        .unwrap();
        let l = p.lineas_valoradas("P").unwrap();
        assert_eq!((l[2].precio, l[2].importe), (dec!(20.00), dec!(2.00)));
        // %CI sin máscara: 3 % sobre 20 + 100 + 2 = 122 → 3,66
        assert_eq!((l[3].precio, l[3].importe), (dec!(122.00), dec!(3.66)));
        assert_eq!(p.precio("P").unwrap(), dec!(125.66));
        assert_eq!(mascara_porcentaje("%MA"), "");
        assert_eq!(mascara_porcentaje("MO&PERD"), "MO");
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
