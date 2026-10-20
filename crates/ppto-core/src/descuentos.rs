// SPDX-License-Identifier: GPL-3.0-or-later
//! Descuentos de fabricante sobre tarifa.
//!
//! Un recurso puede guardar su **PVP de tarifa** y su **fabricante**
//! ([`PrecioTarifa`]). Su precio neto es `PVP × (100 − descuento) / 100`,
//! redondeado a los decimales de precio, con el descuento del fabricante
//! en el presupuesto (p. ej. KOOLAIR 45 %). Al cambiar el descuento de un
//! fabricante se recalculan todos sus recursos de una vez.
//!
//! El precio neto se guarda en `precio`, de modo que el resto del motor,
//! los informes y el BC3 (que solo lleva netos) no cambian.

use crate::concepto::{Concepto, LineaDescomposicion, Naturaleza, PrecioTarifa};
use crate::error::ErrorMotor;
use crate::presupuesto::Presupuesto;
use crate::redondear;
use rust_decimal::Decimal;
use std::collections::BTreeMap;

/// Nombre normalizado del fabricante (mayúsculas, sin espacios sobrantes).
pub fn normalizar(fabricante: &str) -> String {
    fabricante
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

fn validar_pct(pct: Decimal) -> Result<(), ErrorMotor> {
    if pct < Decimal::ZERO || pct >= Decimal::ONE_HUNDRED {
        return Err(ErrorMotor::ValorInvalido(
            "el descuento debe estar entre 0 y 99,99 %".into(),
        ));
    }
    Ok(())
}

/// Clave de la tabla de descuentos: «S&P» (general del fabricante) o
/// «S&P/E08» (grupo de producto).
pub fn clave(fabricante: &str, grupo: Option<&str>) -> String {
    let f = normalizar(fabricante);
    match grupo.map(normalizar).filter(|g| !g.is_empty()) {
        Some(g) => format!("{f}/{g}"),
        None => f,
    }
}

/// Separa una clave «FAB/GRUPO» en fabricante y grupo.
pub fn partes(clave: &str) -> (String, Option<String>) {
    match clave.split_once('/') {
        Some((f, g)) => (normalizar(f), Some(normalizar(g)).filter(|g| !g.is_empty())),
        None => (normalizar(clave), None),
    }
}

/// Una fila de la tabla de descuentos.
#[derive(Debug, Clone, PartialEq)]
pub struct Fabricante {
    /// Clave («S&P» o «S&P/E08»).
    pub nombre: String,
    pub fabricante: String,
    pub grupo: Option<String>,
    pub descuento: Decimal,
    /// Recursos a los que se aplica este descuento.
    pub recursos: usize,
}

impl Presupuesto {
    /// Descuento general del fabricante (0 si no tiene).
    pub fn descuento(&self, fabricante: &str) -> Decimal {
        self.descuentos
            .get(&normalizar(fabricante))
            .copied()
            .unwrap_or_default()
    }

    /// Clave del descuento que se aplica a una tarifa: la del grupo si
    /// tiene descuento propio; si no, la general del fabricante.
    pub fn clave_aplicada(&self, t: &PrecioTarifa) -> String {
        if let Some(g) = &t.grupo {
            let k = clave(&t.fabricante, Some(g));
            if self.descuentos.contains_key(&k) {
                return k;
            }
        }
        normalizar(&t.fabricante)
    }

    /// Descuento que se aplica a una tarifa (grupo o general).
    pub fn descuento_tarifa(&self, t: &PrecioTarifa) -> Decimal {
        self.descuentos
            .get(&self.clave_aplicada(t))
            .copied()
            .unwrap_or_default()
    }

    /// Precio neto a partir de un PVP y un descuento.
    pub fn neto(&self, pvp: Decimal, descuento: Decimal) -> Decimal {
        redondear(
            pvp * (Decimal::ONE_HUNDRED - descuento) / Decimal::ONE_HUNDRED,
            self.decimales.precio,
        )
    }

    /// Liga un recurso básico a la tarifa de un fabricante (sin grupo).
    pub fn fijar_tarifa(&mut self, codigo: &str, fabricante: &str, pvp: Decimal) -> Result<(), ErrorMotor> {
        self.fijar_tarifa_grupo(codigo, fabricante, None, pvp)
    }

    /// Liga un recurso básico a la tarifa de un fabricante y grupo de
    /// producto, y recalcula su neto con el descuento que le toca.
    pub fn fijar_tarifa_grupo(
        &mut self,
        codigo: &str,
        fabricante: &str,
        grupo: Option<&str>,
        pvp: Decimal,
    ) -> Result<(), ErrorMotor> {
        let fab = normalizar(fabricante);
        if fab.is_empty() || fab.contains('/') {
            return Err(ErrorMotor::ValorInvalido("fabricante no válido".into()));
        }
        if pvp < Decimal::ZERO {
            return Err(ErrorMotor::ValorInvalido("el PVP no puede ser negativo".into()));
        }
        let t = PrecioTarifa {
            fabricante: fab,
            grupo: grupo.map(normalizar).filter(|g| !g.is_empty()),
            pvp,
        };
        let neto = self.neto(pvp, self.descuento_tarifa(&t));
        let c = self.concepto_mut(codigo)?;
        if !c.naturaleza.es_basico() {
            return Err(ErrorMotor::ValorInvalido(format!(
                "«{codigo}» no es un recurso básico: no tiene precio de tarifa"
            )));
        }
        c.tarifa = Some(t);
        c.precio = neto;
        Ok(())
    }

    /// Cambia el grupo de producto de un recurso con tarifa (vacío = sin grupo).
    pub fn fijar_grupo(&mut self, codigo: &str, grupo: &str) -> Result<(), ErrorMotor> {
        let t = self
            .concepto(codigo)?
            .tarifa
            .clone()
            .ok_or_else(|| ErrorMotor::ValorInvalido(format!("«{codigo}» no tiene tarifa")))?;
        self.fijar_tarifa_grupo(codigo, &t.fabricante, Some(grupo), t.pvp)
    }

    /// Recalcula el neto de todos los recursos con tarifa.
    fn recalcular_tarifas(&mut self) {
        let netos: Vec<(String, Decimal)> = self
            .conceptos
            .values()
            .filter_map(|c| {
                c.tarifa
                    .as_ref()
                    .map(|t| (c.codigo.clone(), self.neto(t.pvp, self.descuento_tarifa(t))))
            })
            .collect();
        for (c, n) in netos {
            if let Some(x) = self.conceptos.get_mut(&c) {
                x.precio = n;
            }
        }
    }

    fn aplicados(&self, k: &str) -> Vec<String> {
        self.conceptos
            .values()
            .filter(|c| c.tarifa.as_ref().is_some_and(|t| self.clave_aplicada(t) == k))
            .map(|c| c.codigo.clone())
            .collect()
    }

    /// Fija el descuento de un fabricante («S&P») o de un grupo de producto
    /// («S&P/E08») y recalcula. Devuelve los recursos a los que se aplica.
    pub fn fijar_descuento(&mut self, clave_dto: &str, pct: Decimal) -> Result<Vec<String>, ErrorMotor> {
        validar_pct(pct)?;
        let (f, g) = partes(clave_dto);
        if f.is_empty() {
            return Err(ErrorMotor::ValorInvalido("falta el fabricante".into()));
        }
        let k = clave(&f, g.as_deref());
        self.descuentos.insert(k.clone(), pct);
        self.recalcular_tarifas();
        Ok(self.aplicados(&k))
    }

    /// Quita un descuento (los recursos de un grupo vuelven al general del
    /// fabricante; los del fabricante, a 0 %).
    pub fn quitar_descuento(&mut self, clave_dto: &str) -> Vec<String> {
        let (f, g) = partes(clave_dto);
        self.descuentos.remove(&clave(&f, g.as_deref()));
        let antes: Vec<String> = self
            .conceptos
            .values()
            .filter(|c| c.tarifa.as_ref().is_some_and(|t| t.fabricante == f))
            .map(|c| c.codigo.clone())
            .collect();
        self.recalcular_tarifas();
        antes
    }

    /// Tabla de descuentos: los definidos y los fabricantes con recursos.
    pub fn fabricantes(&self) -> Vec<Fabricante> {
        let fila = |k: &str, d: Decimal| {
            let (f, g) = partes(k);
            Fabricante {
                nombre: k.to_owned(),
                fabricante: f,
                grupo: g,
                descuento: d,
                recursos: 0,
            }
        };
        let mut m: BTreeMap<String, Fabricante> =
            self.descuentos.iter().map(|(k, d)| (k.clone(), fila(k, *d))).collect();
        for c in self.conceptos.values() {
            if let Some(t) = &c.tarifa {
                let k = self.clave_aplicada(t);
                m.entry(k.clone()).or_insert_with(|| fila(&k, Decimal::ZERO)).recursos += 1;
            }
        }
        m.into_values().collect()
    }

    /// Copia del presupuesto con los descuentos **a la vista** en los
    /// descompuestos: cada recurso con tarifa y descuento pasa a su PVP y,
    /// detrás de cada línea suya, se añade una línea porcentual negativa
    /// «Descuento FABRICANTE −x %» cuya máscara es el código del recurso
    /// (`<código>%`), de modo que solo se aplica a él. Es lo que se exporta al
    /// BC3 para que Presto muestre PVP y descuento.
    ///
    /// Los recursos que no se pueden mostrar así se dejan a neto y se avisa:
    /// cuelgan directamente de un capítulo, su código tiene «%» o «&» o pasa
    /// de 19 caracteres, u otra línea del mismo descompuesto empieza igual.
    /// El importe puede diferir algún céntimo del neto por el redondeo de la
    /// línea porcentual.
    pub fn con_descuentos_visibles(&self) -> (Presupuesto, Vec<String>) {
        self.con_descuentos_visibles_hasta(20)
    }

    /// Como [`Self::con_descuentos_visibles`], con códigos de `max` caracteres
    /// como mucho: la línea `<código>%` tiene que caber.
    pub fn con_descuentos_visibles_hasta(&self, max: usize) -> (Presupuesto, Vec<String>) {
        let mut p = self.clone();
        let mut avisos = Vec::new();
        // Recursos con descuento > 0 y si se pueden mostrar.
        let mut candidatos: BTreeMap<String, (Decimal, String)> = BTreeMap::new();
        for c in self.conceptos.values() {
            let Some(t) = &c.tarifa else { continue };
            let d = self.descuento_tarifa(t);
            if d.is_zero() {
                continue;
            }
            if c.codigo.contains(['%', '&']) || c.codigo.chars().count() + 1 > max {
                avisos.push(format!(
                    "«{}» se exporta a neto: su código no admite línea de descuento (tiene % o & o pasa de {} caracteres)",
                    c.codigo,
                    max.saturating_sub(1)
                ));
                continue;
            }
            candidatos.insert(c.codigo.clone(), (d, self.clave_aplicada(t)));
        }
        // No se puede si cuelga de un capítulo o si la máscara alcanza a otra línea.
        for c in self.conceptos.values() {
            for l in &c.descomposicion {
                if !candidatos.contains_key(&l.hijo) {
                    continue;
                }
                let choca = c
                    .descomposicion
                    .iter()
                    .any(|o| o.hijo != l.hijo && o.hijo.starts_with(&l.hijo));
                if c.naturaleza == Naturaleza::Capitulo || choca {
                    avisos.push(format!(
                        "«{}» se exporta a neto: {}",
                        l.hijo,
                        if choca {
                            format!("en «{}» otra línea empieza por el mismo código", c.codigo)
                        } else {
                            format!("cuelga directamente del capítulo «{}»", c.codigo)
                        }
                    ));
                    candidatos.remove(&l.hijo);
                }
            }
        }
        // Solo los que se usan en algún descompuesto (sin conceptos huérfanos).
        candidatos.retain(|k, _| {
            self.conceptos
                .values()
                .any(|c| c.descomposicion.iter().any(|l| &l.hijo == k))
        });
        if candidatos.is_empty() {
            return (p, avisos);
        }
        // Recursos a PVP y una línea de descuento por recurso.
        for (codigo, (d, clave)) in &candidatos {
            if let Some(c) = p.conceptos.get_mut(codigo)
                && let Some(t) = &c.tarifa
            {
                c.precio = t.pvp;
            }
            let pc = format!("{codigo}%");
            let resumen = format!("Descuento {clave} {} %", d.normalize());
            p.conceptos.insert(pc.clone(), Concepto::porcentaje(pc, resumen));
        }
        for c in p.conceptos.values_mut() {
            if c.naturaleza == Naturaleza::Capitulo
                || !c.descomposicion.iter().any(|l| candidatos.contains_key(&l.hijo))
            {
                continue;
            }
            let mut nuevas = Vec::with_capacity(c.descomposicion.len() + 2);
            for l in c.descomposicion.drain(..) {
                let dto = candidatos.get(&l.hijo).map(|x| x.0);
                let pc = format!("{}%", l.hijo);
                nuevas.push(l);
                if let Some(d) = dto {
                    nuevas.push(LineaDescomposicion::new(pc, -d));
                }
            }
            c.descomposicion = nuevas;
        }
        p.ajustar_redondeo_descuentos(self, &candidatos.keys().map(|k| format!("{k}%")).collect());
        (p, avisos)
    }

    /// Con los descuentos a la vista, el neto ya no se redondea por unidad
    /// sino sobre el importe de la línea, y una partida puede variar algún
    /// céntimo. Para que el BC3 valga lo mismo que la obra, se añade antes
    /// de los medios auxiliares una línea «Ajuste de redondeo» (0,01 € ×
    /// céntimos) en las partidas que lo necesitan.
    fn ajustar_redondeo_descuentos(&mut self, original: &Presupuesto, lineas_dto: &std::collections::BTreeSet<String>) {
        const AJUSTE: &str = "AJ.REDONDEO";
        let Ok(vo) = original.valorar() else { return };
        let un_centimo = Decimal::new(1, 2);
        // Base (suma de líneas antes del primer porcentaje que no sea descuento)
        let base = |p: &Presupuesto, lineas: &[crate::presupuesto::LineaValorada]| {
            lineas
                .iter()
                .take_while(|l| {
                    lineas_dto.contains(&l.hijo)
                        || p.conceptos
                            .get(&l.hijo)
                            .is_none_or(|c| c.naturaleza != Naturaleza::Porcentaje)
                })
                .map(|l| l.importe)
                .sum::<Decimal>()
        };
        // De abajo arriba: solo se ajusta una partida cuando sus hijas ya cuadran
        for _ in 0..6 {
            let Ok(vq) = self.valorar() else { return };
            let distinto = |c: &str| vo.precios.get(c) != vq.precios.get(c);
            let mut ajustes = Vec::new();
            for (codigo, c) in &self.conceptos {
                if c.naturaleza == Naturaleza::Capitulo || c.descomposicion.is_empty() || !distinto(codigo) {
                    continue;
                }
                let hijas_ok = c.descomposicion.iter().all(|l| {
                    self.conceptos.get(&l.hijo).is_none_or(|h| h.descomposicion.is_empty()) || !distinto(&l.hijo)
                });
                if !hijas_ok {
                    continue;
                }
                let (Some(lo), Some(lq)) = (vo.lineas.get(codigo), vq.lineas.get(codigo)) else {
                    continue;
                };
                let a = base(original, lo) - base(self, lq);
                if !a.is_zero() {
                    ajustes.push((codigo.clone(), a));
                }
            }
            if ajustes.is_empty() {
                return;
            }
            if !self.conceptos.contains_key(AJUSTE) {
                let mut c = Concepto::basico(
                    AJUSTE,
                    "€",
                    "Ajuste de redondeo de descuentos",
                    Naturaleza::Otros,
                    un_centimo,
                );
                c.texto = Some(
                    "Céntimos de diferencia entre aplicar el descuento al precio unitario o al importe de la línea."
                        .into(),
                );
                self.conceptos.insert(AJUSTE.into(), c);
            }
            for (codigo, a) in ajustes {
                let Some(c) = self.conceptos.get_mut(&codigo) else {
                    continue;
                };
                let rend = a / un_centimo;
                if let Some(l) = c.descomposicion.iter_mut().find(|l| l.hijo == AJUSTE) {
                    l.rendimiento += rend;
                    continue;
                }
                let pos = c
                    .descomposicion
                    .iter()
                    .position(|l| {
                        !lineas_dto.contains(&l.hijo)
                            && original
                                .conceptos
                                .get(&l.hijo)
                                .is_some_and(|h| h.naturaleza == Naturaleza::Porcentaje)
                    })
                    .unwrap_or(c.descomposicion.len());
                c.descomposicion.insert(pos, LineaDescomposicion::new(AJUSTE, rend));
            }
        }
    }

    /// Inverso de [`Self::con_descuentos_visibles`]: al leer un BC3 que trae
    /// los descuentos a la vista (recurso a PVP seguido de la línea
    /// `<código>%` «Descuento FABRICANTE x %»), los convierte otra vez en
    /// tarifa del recurso y descuento del fabricante, y quita esas líneas.
    /// El importe de la obra no cambia (salvo algún céntimo de redondeo).
    /// Devuelve los fabricantes recuperados y los avisos.
    pub fn recuperar_descuentos_visibles(&mut self) -> Vec<String> {
        let mut avisos = Vec::new();
        // (recurso, línea %, clave, descuento)
        let mut hallados: Vec<(String, String, String, Decimal)> = Vec::new();
        for c in self.conceptos.values() {
            if c.naturaleza != Naturaleza::Porcentaje {
                continue;
            }
            let Some(recurso) = c.codigo.strip_suffix('%') else {
                continue;
            };
            if recurso.is_empty() || recurso.contains(['%', '&']) {
                continue;
            }
            let Some((clave, d)) = leer_resumen_descuento(&c.resumen) else {
                continue;
            };
            if !self.conceptos.get(recurso).is_some_and(|r| r.naturaleza.es_basico()) {
                continue;
            }
            // Todas sus líneas deben ser el mismo −d
            let coherente = self
                .conceptos
                .values()
                .flat_map(|p| p.descomposicion.iter())
                .filter(|l| l.hijo == c.codigo)
                .all(|l| l.rendimiento == -d);
            if !coherente {
                avisos.push(format!(
                    "«{}»: líneas de descuento distintas entre partidas; se deja como está",
                    c.codigo
                ));
                continue;
            }
            hallados.push((recurso.to_owned(), c.codigo.clone(), clave, d));
        }
        // Un descuento por clave
        let mut por_clave: BTreeMap<String, Decimal> = BTreeMap::new();
        hallados.retain(|(r, _, k, d)| match por_clave.get(k) {
            Some(x) if x != d => {
                avisos.push(format!(
                    "«{r}»: {k} con {d} % y {x} % en el mismo BC3; se deja como está"
                ));
                false
            }
            _ => {
                por_clave.insert(k.clone(), *d);
                true
            }
        });
        for (k, d) in &por_clave {
            self.descuentos.entry(k.clone()).or_insert(*d);
        }
        let mut n = 0;
        for (recurso, linea, clave, _) in &hallados {
            let pvp = self.conceptos[recurso].precio;
            let (f, g) = partes(clave);
            if self.fijar_tarifa_grupo(recurso, &f, g.as_deref(), pvp).is_err() {
                continue;
            }
            for c in self.conceptos.values_mut() {
                c.descomposicion.retain(|l| &l.hijo != linea);
            }
            self.conceptos.remove(linea);
            n += 1;
        }
        // El ajuste de redondeo de la exportación ya no hace falta: el neto
        // vuelve a redondearse por unidad
        if n > 0
            && self
                .conceptos
                .get("AJ.REDONDEO")
                .is_some_and(|c| c.resumen == "Ajuste de redondeo de descuentos")
        {
            for c in self.conceptos.values_mut() {
                c.descomposicion.retain(|l| l.hijo != "AJ.REDONDEO");
            }
            self.conceptos.remove("AJ.REDONDEO");
        }
        if n > 0 {
            avisos.insert(
                0,
                format!(
                    "descuentos por fabricante recuperados del BC3: {} ({n} recursos a PVP con su descuento)",
                    por_clave
                        .iter()
                        .map(|(k, d)| format!("{k} {} %", d.normalize()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }
        avisos
    }
}

/// «Descuento ITALSAN 50 %» → («ITALSAN», 50).
fn leer_resumen_descuento(resumen: &str) -> Option<(String, Decimal)> {
    let r = resumen.trim().strip_prefix("Descuento ")?.strip_suffix('%')?.trim_end();
    let (clave, num) = r.rsplit_once(' ')?;
    let d: Decimal = num.replace(',', ".").parse().ok()?;
    let clave = clave.trim();
    (!clave.is_empty() && d > Decimal::ZERO && d < Decimal::ONE_HUNDRED).then(|| {
        let (f, g) = partes(clave);
        (self::clave(&f, g.as_deref()), d)
    })
}
