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
//!   (si la IA proponía otro, se informa en `precios_distintos`). Si existen
//!   a neto sin tarifa y la propuesta trae PVP y fabricante, se **ligan** a
//!   la tarifa (`ligados`), y su neto pasa a depender del descuento.
//! - Los nuevos se dan de alta con la naturaleza, unidad y precio propuestos.
//! - El descompuesto de la partida se sustituye por el propuesto.
//!
//! - Si la propuesta trae `nueva` y la partida no existe, se crea antes en el
//!   capítulo indicado (y el capítulo, si falta, detrás de `despues_de`), con
//!   su unidad, resumen, texto y cantidad: es un contradictorio o adicional.
//!
//! Una propuesta puede ser también una **lista de cambios** de la obra
//! ([`Contenido::Cambios`]): quitar partidas o capítulos, cambiar la cantidad
//! (o la hoja de medición) de una partida en un capítulo, rehacer su resumen,
//! texto o unidad, y moverla de capítulo. Se aplican todos o ninguno.
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
    /// Precio neto (si no se da PVP).
    #[serde(default)]
    pub precio: Option<Decimal>,
    /// PVP de tarifa: el neto sale del descuento del fabricante.
    #[serde(default)]
    pub pvp: Option<Decimal>,
    #[serde(default)]
    pub fabricante: Option<String>,
    /// Grupo de producto del fabricante (descuento propio, p. ej. «E08»).
    #[serde(default)]
    pub grupo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Contenido {
    Descompuesto(Vec<LineaPropuesta>),
    PrecioDirecto(Decimal),
    /// Cambios de la obra (no de precio), en orden.
    Cambios(Vec<Cambio>),
}

/// Cambio de la obra propuesto por la IA. `capitulo` es el capítulo padre
/// de la línea; si se deja vacío y la partida solo está en un sitio, se usa ese.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Cambio {
    /// Quita la partida (o el capítulo) del capítulo y borra lo que quede sin usar.
    Borrar {
        #[serde(default)]
        capitulo: String,
        partida: String,
    },
    /// Cambia la cantidad de la partida en el capítulo. Con `medicion`, la
    /// cantidad sale de la hoja de medición; sin ella, se quita la hoja.
    Cantidad {
        #[serde(default)]
        capitulo: String,
        partida: String,
        cantidad: Decimal,
        #[serde(default)]
        medicion: Vec<crate::medicion::LineaMedicion>,
    },
    /// Cambia el resumen, el texto descriptivo o la unidad (los que vengan).
    Texto {
        partida: String,
        #[serde(default)]
        resumen: Option<String>,
        #[serde(default)]
        texto: Option<String>,
        #[serde(default)]
        unidad: Option<String>,
    },
    /// Mueve la partida (o el capítulo) a otro capítulo, delante de `antes_de`
    /// o al final.
    Mover {
        #[serde(default)]
        de: String,
        partida: String,
        a: String,
        #[serde(default)]
        antes_de: Option<String>,
    },
}

impl Cambio {
    pub fn partida(&self) -> &str {
        match self {
            Self::Borrar { partida, .. }
            | Self::Cantidad { partida, .. }
            | Self::Texto { partida, .. }
            | Self::Mover { partida, .. } => partida,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstadoPropuesta {
    Pendiente,
    Aceptada,
    Rechazada,
}

/// Partida que no está en el presupuesto (contradictorio o adicional): se
/// crea al aceptar la propuesta.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NuevaPartida {
    /// Capítulo donde se cuelga (se crea en la raíz si no existe).
    pub capitulo: String,
    #[serde(default)]
    pub capitulo_resumen: String,
    /// Capítulo de la raíz detrás del que se coloca el capítulo nuevo.
    #[serde(default)]
    pub despues_de: Option<String>,
    pub unidad: String,
    pub resumen: String,
    #[serde(default)]
    pub texto: Option<String>,
    pub cantidad: Decimal,
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
    /// Descuentos estimados por fabricante (puntos). Solo se usan para los
    /// fabricantes que la obra aún no tiene: los del usuario mandan.
    #[serde(default)]
    pub descuentos: std::collections::BTreeMap<String, Decimal>,
    /// Partida nueva (contradictorio): se crea al aceptar.
    #[serde(default)]
    pub nueva: Option<NuevaPartida>,
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
    /// Descuentos de fabricante que la obra no tenía y se toman de la propuesta.
    pub descuentos_nuevos: Vec<(String, Decimal)>,
    /// Recursos que ya estaban en la obra a neto y se ligan a la tarifa del
    /// fabricante: (código, neto antes, neto con el descuento).
    pub ligados: Vec<(String, Decimal, Decimal)>,
    /// Cambios de la obra aplicados, explicados («Borra P01.02 de C01…»).
    pub cambios: Vec<String>,
    /// PEM antes y después (solo en propuestas de cambios).
    pub pem_antes: Decimal,
    pub pem_despues: Decimal,
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

    /// Crea la partida de un contradictorio (y su capítulo si falta).
    fn crear_partida_nueva(&mut self, partida: &str, n: &NuevaPartida) -> Result<(), ErrorMotor> {
        crate::edicion::validar_codigo(partida)?;
        if n.unidad.trim().is_empty() || n.resumen.trim().is_empty() {
            return Err(ErrorMotor::ValorInvalido(format!(
                "«{partida}» nueva sin unidad o sin resumen"
            )));
        }
        if n.cantidad <= Decimal::ZERO {
            return Err(ErrorMotor::ValorInvalido(format!("«{partida}» nueva sin cantidad")));
        }
        match self.conceptos.get(&n.capitulo) {
            Some(c) if c.naturaleza != Naturaleza::Capitulo => {
                return Err(ErrorMotor::ValorInvalido(format!("«{}» no es un capítulo", n.capitulo)));
            }
            Some(_) => {}
            None => {
                let resumen = if n.capitulo_resumen.trim().is_empty() {
                    "ADICIONALES"
                } else {
                    n.capitulo_resumen.trim()
                };
                let raiz = self.raiz.clone();
                self.nuevo_capitulo(&raiz, &n.capitulo, resumen)?;
                if let Some(tras) = &n.despues_de {
                    let lineas = &mut self
                        .conceptos
                        .get_mut(&raiz)
                        .ok_or_else(|| ErrorMotor::ValorInvalido("sin raíz".into()))?
                        .descomposicion;
                    if let Some(i) = lineas.iter().position(|l| &l.hijo == tras)
                        && let Some(nueva) = lineas.pop()
                    {
                        lineas.insert(i + 1, nueva);
                    }
                }
            }
        }
        let mut c = Concepto::partida(partida, n.unidad.trim(), n.resumen.trim(), Vec::new());
        c.texto = n.texto.clone().filter(|t| !t.trim().is_empty());
        self.insertar(c)?;
        self.anadir_linea(&n.capitulo, partida, n.cantidad)
    }

    fn aplicar_propuesta_sin_deshacer(&mut self, prop: &Propuesta) -> Result<Aplicada, ErrorMotor> {
        if let Contenido::Cambios(cambios) = &prop.contenido {
            return self.aplicar_cambios(cambios);
        }
        let partida = prop.partida.as_str();
        if let Some(n) = &prop.nueva
            && !self.conceptos.contains_key(partida)
        {
            self.crear_partida_nueva(partida, n)?;
        }
        let c = self.concepto(partida)?;
        if c.naturaleza != Naturaleza::Partida {
            // Unidad de obra importada sin tipo (p. ej. un ciego de Presto):
            // un concepto «otros» sin descompuesto colgado de un capítulo.
            let en_capitulo = self
                .conceptos
                .values()
                .any(|p| p.naturaleza == Naturaleza::Capitulo && p.descomposicion.iter().any(|l| l.hijo == partida));
            if c.naturaleza == Naturaleza::Otros && c.descomposicion.is_empty() && en_capitulo {
                let c = self
                    .conceptos
                    .get_mut(partida)
                    .ok_or_else(|| ErrorMotor::ValorInvalido(format!("«{partida}» no existe")))?;
                c.naturaleza = Naturaleza::Partida;
            } else {
                return Err(ErrorMotor::ValorInvalido(format!("«{partida}» no es una partida")));
            }
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
            Contenido::Cambios(_) => unreachable!("se aplican antes"),
            Contenido::Descompuesto(lineas) => {
                if lineas.is_empty() {
                    return Err(ErrorMotor::ValorInvalido("el descompuesto propuesto está vacío".into()));
                }
                for (fab, pct) in &prop.descuentos {
                    let (fa, gr) = crate::descuentos::partes(fab);
                    let f = crate::descuentos::clave(&fa, gr.as_deref());
                    if !self.descuentos.contains_key(&f) {
                        self.fijar_descuento(&f, *pct)?;
                        out.descuentos_nuevos.push((f, *pct));
                    }
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
                            let sin_tarifa = h.tarifa.is_none() && h.naturaleza.es_basico();
                            if let (true, Some(pvp), Some(fab)) = (sin_tarifa, l.pvp, &l.fabricante) {
                                // Recurso ya en la obra a neto: se liga a la tarifa del fabricante
                                let antes = h.precio;
                                self.fijar_tarifa_grupo(&l.codigo, fab, l.grupo.as_deref(), pvp)?;
                                let despues = self.concepto(&l.codigo)?.precio;
                                out.ligados.push((l.codigo.clone(), antes, despues));
                            } else if let Some(pp) = l.precio
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
                                    let precio = match (l.pvp, &l.fabricante) {
                                        (Some(_), None) => {
                                            return Err(ErrorMotor::ValorInvalido(format!(
                                                "«{}»: con PVP hay que indicar el fabricante",
                                                l.codigo
                                            )));
                                        }
                                        (Some(pvp), Some(_)) => pvp,
                                        (None, _) => l.precio.ok_or_else(|| {
                                            ErrorMotor::ValorInvalido(format!("«{}» nuevo sin precio", l.codigo))
                                        })?,
                                    };
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
                            if let (Some(pvp), Some(fab)) = (l.pvp, &l.fabricante) {
                                self.fijar_tarifa_grupo(&l.codigo, fab, l.grupo.as_deref(), pvp)?;
                            }
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

impl Presupuesto {
    /// Capítulo del que cuelga `codigo`: el indicado (comprobando que está
    /// ahí) o, si viene vacío, el único donde está.
    fn padre_de(&self, capitulo: &str, codigo: &str) -> Result<String, ErrorMotor> {
        self.concepto(codigo)?;
        let padres = self.padres(codigo);
        let capitulo = capitulo.trim();
        if !capitulo.is_empty() {
            if padres.iter().any(|p| p == capitulo) {
                return Ok(capitulo.to_owned());
            }
            return Err(ErrorMotor::ValorInvalido(format!("«{codigo}» no está en «{capitulo}»")));
        }
        match padres.as_slice() {
            [uno] => Ok(uno.clone()),
            [] => Err(ErrorMotor::ValorInvalido(format!(
                "«{codigo}» no cuelga de ningún capítulo"
            ))),
            varios => Err(ErrorMotor::ValorInvalido(format!(
                "«{codigo}» está en varios sitios ({}): indique el capítulo",
                varios.join(", ")
            ))),
        }
    }

    fn aplicar_cambios(&mut self, cambios: &[Cambio]) -> Result<Aplicada, ErrorMotor> {
        if cambios.is_empty() {
            return Err(ErrorMotor::ValorInvalido("la propuesta no trae cambios".into()));
        }
        let mut out = Aplicada {
            pem_antes: self.pem()?,
            ..Aplicada::default()
        };
        for c in cambios {
            let t = self.aplicar_cambio(c)?;
            out.cambios.push(t);
        }
        self.validar()?;
        out.pem_despues = self.pem()?;
        Ok(out)
    }

    fn aplicar_cambio(&mut self, c: &Cambio) -> Result<String, ErrorMotor> {
        match c {
            Cambio::Borrar { capitulo, partida } => {
                if *partida == self.raiz {
                    return Err(ErrorMotor::ValorInvalido("no se puede borrar la raíz".into()));
                }
                let padre = self.padre_de(capitulo, partida)?;
                let resumen = self.concepto(partida)?.resumen.clone();
                let borrados = self.quitar_y_purgar(&padre, partida)?;
                let mut t = format!("Quita {partida} «{resumen}» de {padre}");
                if borrados.len() > 1 {
                    t.push_str(&format!(" (y {} conceptos que quedan sin usar)", borrados.len() - 1));
                }
                Ok(t)
            }
            Cambio::Cantidad {
                capitulo,
                partida,
                cantidad,
                medicion,
            } => {
                let padre = self.padre_de(capitulo, partida)?;
                if self.concepto(&padre)?.naturaleza != Naturaleza::Capitulo {
                    return Err(ErrorMotor::ValorInvalido(format!(
                        "«{padre}» no es un capítulo: el rendimiento de un descompuesto se cambia con una propuesta de precio"
                    )));
                }
                let antes = self
                    .concepto(&padre)?
                    .descomposicion
                    .iter()
                    .find(|l| l.hijo == *partida)
                    .map(LineaDescomposicion::cantidad)
                    .unwrap_or_default();
                if medicion.is_empty() {
                    if *cantidad < Decimal::ZERO {
                        return Err(ErrorMotor::ValorInvalido(format!("«{partida}» con cantidad negativa")));
                    }
                    self.mediciones.remove(&(padre.clone(), partida.clone()));
                    self.fijar_rendimiento(&padre, partida, *cantidad)?;
                } else {
                    self.actualizar_medicion(&padre, partida, crate::medicion::Medicion::new(medicion.clone()))?;
                    let total = self
                        .concepto(&padre)?
                        .descomposicion
                        .iter()
                        .find(|l| l.hijo == *partida)
                        .map(LineaDescomposicion::cantidad)
                        .unwrap_or_default();
                    if total != *cantidad {
                        return Err(ErrorMotor::ValorInvalido(format!(
                            "la medición de «{partida}» suma {total}, no {cantidad}"
                        )));
                    }
                }
                let ud = self.concepto(partida)?.unidad.clone();
                Ok(format!(
                    "{partida} en {padre}: cantidad {} → {} {ud}{}",
                    antes.normalize(),
                    cantidad.normalize(),
                    if medicion.is_empty() {
                        String::new()
                    } else {
                        format!(" ({} líneas de medición)", medicion.len())
                    }
                ))
            }
            Cambio::Texto {
                partida,
                resumen,
                texto,
                unidad,
            } => {
                if resumen.is_none() && texto.is_none() && unidad.is_none() {
                    return Err(ErrorMotor::ValorInvalido(format!(
                        "«{partida}»: indique resumen, texto o unidad"
                    )));
                }
                let mut partes = Vec::new();
                if let Some(u) = unidad {
                    if u.trim().is_empty() {
                        return Err(ErrorMotor::ValorInvalido(format!("«{partida}» con unidad vacía")));
                    }
                    let antes = self.concepto(partida)?.unidad.clone();
                    self.fijar_unidad(partida, u)?;
                    partes.push(format!("unidad {antes} → {}", u.trim()));
                }
                let c = self.concepto_mut(partida)?;
                if let Some(r) = resumen {
                    if r.trim().is_empty() {
                        return Err(ErrorMotor::ValorInvalido(format!("«{partida}» con resumen vacío")));
                    }
                    partes.push(format!("resumen «{}» → «{}»", c.resumen, r.trim()));
                    c.resumen = r.trim().to_owned();
                }
                if let Some(t) = texto {
                    c.texto = Some(t.trim().to_owned()).filter(|t| !t.is_empty());
                    partes.push("texto descriptivo nuevo".into());
                }
                Ok(format!("{partida}: {}", partes.join("; ")))
            }
            Cambio::Mover {
                de,
                partida,
                a,
                antes_de,
            } => {
                let origen = self.padre_de(de, partida)?;
                if self.concepto(a)?.naturaleza != Naturaleza::Capitulo {
                    return Err(ErrorMotor::ValorInvalido(format!("«{a}» no es un capítulo")));
                }
                if let Some(x) = antes_de
                    && !self.concepto(a)?.descomposicion.iter().any(|l| &l.hijo == x)
                {
                    return Err(ErrorMotor::ValorInvalido(format!("«{x}» no está en «{a}»")));
                }
                self.trasladar_linea(&origen, partida, a, antes_de.as_deref())?;
                Ok(match antes_de {
                    Some(x) => format!("Mueve {partida} de {origen} a {a}, delante de {x}"),
                    None => format!("Mueve {partida} de {origen} al final de {a}"),
                })
            }
        }
    }
}
