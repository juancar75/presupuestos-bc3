// SPDX-License-Identifier: GPL-3.0-or-later
//! Lector FIEBDC-3.
//!
//! Estructura del formato: registros que empiezan por `~` y una letra
//! (`~C`, `~D`…), campos separados por `|` y subcampos por `\`. Los saltos de
//! línea no tienen significado salvo dentro de textos.

use crate::{Cabecera, Discrepancia, ErrorBc3, Gravedad, Importacion, Incidencia, Porcentajes};
use ppto_core::concepto::{Concepto, LineaDescomposicion, Naturaleza};
use ppto_core::formato::num;
use ppto_core::medicion::{LineaMedicion, Medicion, TipoLinea};
use ppto_core::presupuesto::OpcionesCi;
use ppto_core::{Decimal, Decimales, ErrorMotor, Presupuesto, redondear};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::str::FromStr;

/// Código del capítulo raíz que se crea cuando el BC3 no declara uno (`##`).
const RAIZ_SINTETICA: &str = "RAIZ";

pub fn importar_fichero(ruta: impl AsRef<Path>) -> Result<Importacion, ErrorBc3> {
    let bytes = std::fs::read(ruta).map_err(ErrorBc3::Lectura)?;
    importar(&bytes)
}

pub fn importar(bytes: &[u8]) -> Result<Importacion, ErrorBc3> {
    let mut l = Lector::default();
    let texto = l.decodificar(bytes);
    l.leer_registros(&texto);
    l.construir()
}

// ---------------------------------------------------------------------------

struct ConceptoBc3 {
    codigo: String,
    unidad: String,
    resumen: String,
    precio: Option<Decimal>,
    tipo: String,
    capitulo: bool,
    raiz: bool,
}

struct LineaD {
    hijo: String,
    factor: Option<Decimal>,
    rendimiento: Option<Decimal>,
    linea: usize,
}

struct MedicionBc3 {
    padre: Option<String>,
    hijo: String,
    total: Option<Decimal>,
    lineas: Vec<LineaMedicion>,
    linea: usize,
}

#[derive(Default)]
struct Lector {
    cabecera: Cabecera,
    porcentajes: Porcentajes,
    registros: BTreeMap<String, usize>,
    incidencias: Vec<Incidencia>,
    conceptos: BTreeMap<String, ConceptoBc3>,
    orden: Vec<String>,
    desc: BTreeMap<String, Vec<LineaD>>,
    textos: HashMap<String, String>,
    mediciones: Vec<MedicionBc3>,
}

impl Lector {
    fn anotar(&mut self, gravedad: Gravedad, linea: usize, registro: &str, mensaje: impl Into<String>) {
        self.incidencias.push(Incidencia {
            gravedad,
            linea,
            registro: registro.to_owned(),
            mensaje: mensaje.into(),
        });
    }

    // ------------------------------------------------------------ codificación

    fn decodificar(&mut self, bytes: &[u8]) -> String {
        let declarado = juego_declarado(bytes).to_uppercase();
        let declarado = declarado.trim();
        let ascii = bytes.is_ascii();
        let texto = match declarado {
            "UTF-8" | "UTF8" => match std::str::from_utf8(bytes) {
                Ok(s) => s.to_owned(),
                Err(_) => {
                    self.anotar(
                        Gravedad::Error,
                        0,
                        "",
                        "el fichero declara UTF-8 pero contiene bytes no válidos; se han sustituido por «�»",
                    );
                    String::from_utf8_lossy(bytes).into_owned()
                }
            },
            "" if !ascii && std::str::from_utf8(bytes).is_ok() => {
                self.anotar(
                    Gravedad::Info,
                    0,
                    "",
                    "sin juego de caracteres declarado: se ha leído como UTF-8",
                );
                String::from_utf8_lossy(bytes).into_owned()
            }
            "" | "ANSI" | "1252" | "WINDOWS-1252" | "CP1252" => win1252(bytes),
            "850" | "437" => {
                self.anotar(
                    Gravedad::Aviso,
                    0,
                    "",
                    format!("juego de caracteres {declarado} (MS-DOS) leído como ANSI: revise tildes y eñes"),
                );
                win1252(bytes)
            }
            otro => {
                self.anotar(
                    Gravedad::Aviso,
                    0,
                    "",
                    format!("juego de caracteres desconocido «{otro}»: se ha leído como ANSI"),
                );
                win1252(bytes)
            }
        };
        texto.replace('\u{1a}', "")
    }

    // --------------------------------------------------------------- registros

    fn leer_registros(&mut self, texto: &str) {
        let mut linea = 1usize;
        let mut ultimo = 0usize;
        let inicios: Vec<usize> = texto.match_indices('~').map(|(i, _)| i).collect();
        if inicios.first().is_some_and(|&i| !texto[..i].trim().is_empty()) {
            self.anotar(Gravedad::Aviso, 1, "", "hay texto antes del primer registro: se ignora");
        }
        for (k, &ini) in inicios.iter().enumerate() {
            linea += texto[ultimo..ini].matches('\n').count();
            ultimo = ini;
            let fin = inicios.get(k + 1).copied().unwrap_or(texto.len());
            let reg = texto[ini + 1..fin].trim_end_matches(['\r', '\n', ' ', '\t']);
            self.registro(reg, linea);
        }
    }

    fn registro(&mut self, reg: &str, linea: usize) {
        let campos: Vec<&str> = reg.split('|').collect();
        let tipo = campos[0].trim().to_uppercase();
        *self.registros.entry(tipo.clone()).or_insert(0) += 1;
        let campo = |i: usize| campos.get(i).copied().unwrap_or("");
        match tipo.as_str() {
            "V" => {
                let fecha_version: Vec<&str> = campo(2).split('\\').collect();
                self.cabecera = Cabecera {
                    propiedad: campo(1).trim().to_owned(),
                    version_formato: fecha_version[0].trim().to_owned(),
                    programa: campo(3).trim().to_owned(),
                    rotulo: campo(4).split('\\').next().unwrap_or("").trim().to_owned(),
                    juego_caracteres: campo(5).trim().to_owned(),
                };
            }
            "K" => self.registro_k(&campos, linea),
            "C" => self.registro_c(&campos, linea),
            "D" | "Y" => self.registro_d(&campos, linea, tipo == "Y"),
            "T" => {
                let codigo = normalizar(campo(1));
                let texto = campo(2).trim().to_owned();
                if codigo.is_empty() {
                    self.anotar(Gravedad::Error, linea, "T", "texto sin código de concepto");
                } else {
                    self.textos.insert(codigo, texto);
                }
            }
            "M" | "N" => self.registro_m(&campos, linea, tipo == "N"),
            "L" | "P" | "Q" | "J" | "W" | "G" | "E" | "O" | "X" | "A" | "F" | "R" | "B" => {}
            "" => self.anotar(Gravedad::Error, linea, "", "registro vacío (carácter «~» suelto)"),
            otro => self.anotar(
                Gravedad::Aviso,
                linea,
                otro,
                format!("tipo de registro desconocido «~{otro}»: se ignora"),
            ),
        }
    }

    fn registro_k(&mut self, campos: &[&str], linea: usize) {
        let sub: Vec<&str> = campos.get(2).copied().unwrap_or("").split('\\').collect();
        let mut leer = |i: usize, nombre: &str| -> Option<Decimal> {
            let s = sub.get(i).copied().unwrap_or("");
            match numero(s) {
                Ok(v) => v,
                Err(_) => {
                    self.anotar(Gravedad::Aviso, linea, "K", format!("{nombre} no numérico «{s}»"));
                    None
                }
            }
        };
        self.porcentajes = Porcentajes {
            costes_indirectos: leer(0, "costes indirectos"),
            gastos_generales: leer(1, "gastos generales"),
            beneficio_industrial: leer(2, "beneficio industrial"),
            baja: leer(3, "baja"),
            iva: leer(4, "IVA"),
        };
    }

    fn registro_c(&mut self, campos: &[&str], linea: usize) {
        let campo = |i: usize| campos.get(i).copied().unwrap_or("");
        let original = campo(1).split('\\').next().unwrap_or("").trim().to_owned();
        let codigo = normalizar(&original);
        if codigo.is_empty() {
            self.anotar(Gravedad::Error, linea, "C", "concepto sin código: se ignora");
            return;
        }
        let precio_txt = campo(4).split('\\').next().unwrap_or("");
        let precio = match numero(precio_txt) {
            Ok(v) => v,
            Err(_) => {
                self.anotar(
                    Gravedad::Error,
                    linea,
                    "C",
                    format!("{codigo}: precio no numérico «{precio_txt}», se toma 0"),
                );
                None
            }
        };
        if self.conceptos.contains_key(&codigo) {
            self.anotar(
                Gravedad::Aviso,
                linea,
                "C",
                format!("{codigo}: concepto repetido, prevalece esta definición"),
            );
        } else {
            self.orden.push(codigo.clone());
        }
        let c = ConceptoBc3 {
            codigo: codigo.clone(),
            unidad: campo(2).trim().to_owned(),
            resumen: campo(3).trim().to_owned(),
            precio,
            tipo: campo(6).trim().to_owned(),
            capitulo: original.ends_with('#'),
            raiz: original.ends_with("##"),
        };
        self.conceptos.insert(codigo, c);
    }

    fn registro_d(&mut self, campos: &[&str], linea: usize, anadir: bool) {
        let reg = if anadir { "Y" } else { "D" };
        let padre = normalizar(campos.get(1).copied().unwrap_or(""));
        if padre.is_empty() {
            self.anotar(
                Gravedad::Error,
                linea,
                reg,
                "descomposición sin código padre: se ignora",
            );
            return;
        }
        let sub: Vec<&str> = campos.get(2).copied().unwrap_or("").split('\\').collect();
        let mut lineas = Vec::new();
        for trozo in sub.chunks(3) {
            let hijo = normalizar(trozo[0]);
            if hijo.is_empty() {
                if trozo.iter().any(|s| !s.trim().is_empty()) {
                    self.anotar(
                        Gravedad::Error,
                        linea,
                        reg,
                        format!("{padre}: línea de descomposición sin código hijo"),
                    );
                }
                continue;
            }
            let mut leer = |i: usize, nombre: &str| -> Option<Decimal> {
                let s = trozo.get(i).copied().unwrap_or("");
                numero(s).unwrap_or_else(|_| {
                    self.anotar(
                        Gravedad::Error,
                        linea,
                        reg,
                        format!("{padre} → {hijo}: {nombre} no numérico «{s}», se toma 1"),
                    );
                    None
                })
            };
            let factor = leer(1, "factor");
            let rendimiento = leer(2, "rendimiento");
            lineas.push(LineaD {
                hijo,
                factor,
                rendimiento,
                linea,
            });
        }
        if anadir {
            self.desc.entry(padre).or_default().extend(lineas);
        } else {
            if self.desc.contains_key(&padre) {
                self.anotar(
                    Gravedad::Aviso,
                    linea,
                    reg,
                    format!("{padre}: descomposición repetida, prevalece esta"),
                );
            }
            self.desc.insert(padre, lineas);
        }
    }

    fn registro_m(&mut self, campos: &[&str], linea: usize, anadir: bool) {
        let reg = if anadir { "N" } else { "M" };
        let campo = |i: usize| campos.get(i).copied().unwrap_or("");
        let codigos: Vec<&str> = campo(1).split('\\').collect();
        let (padre, hijo) = match codigos.as_slice() {
            [p, h, ..] if !h.trim().is_empty() => (Some(normalizar(p)).filter(|p| !p.is_empty()), normalizar(h)),
            [h, ..] => (None, normalizar(h)),
            [] => (None, String::new()),
        };
        if hijo.is_empty() {
            self.anotar(Gravedad::Error, linea, reg, "medición sin código de partida: se ignora");
            return;
        }
        let total = numero(campo(3)).unwrap_or(None);
        let sub: Vec<&str> = campo(4).split('\\').collect();
        let mut lineas = Vec::new();
        for trozo in sub.chunks(6) {
            if trozo.iter().all(|s| s.trim().is_empty()) {
                continue;
            }
            let mut dims = [None; 4];
            for (k, d) in dims.iter_mut().enumerate() {
                let s = trozo.get(k + 2).copied().unwrap_or("");
                *d = numero(s).unwrap_or_else(|_| {
                    self.anotar(
                        Gravedad::Error,
                        linea,
                        reg,
                        format!("{hijo}: dimensión no numérica «{s}», se deja vacía"),
                    );
                    None
                });
            }
            let comentario = trozo.get(1).copied().unwrap_or("").trim().to_owned();
            let [unidades, longitud, anchura, altura] = dims;
            let mut l = LineaMedicion {
                comentario: comentario.clone(),
                unidades,
                longitud,
                anchura,
                altura,
                ..Default::default()
            };
            match trozo[0].trim() {
                "" | "0" => {}
                "1" => l.tipo = TipoLinea::SubtotalParcial,
                "2" => l.tipo = TipoLinea::SubtotalAcumulado,
                "3" => {
                    l.tipo = TipoLinea::Formula;
                    l.formula = Some(comentario);
                    l.comentario = String::new();
                }
                otro => self.anotar(
                    Gravedad::Aviso,
                    linea,
                    reg,
                    format!("{hijo}: tipo de línea de medición «{otro}» desconocido, se trata como normal"),
                ),
            }
            lineas.push(l);
        }
        if anadir
            && let Some(m) = self
                .mediciones
                .iter_mut()
                .rev()
                .find(|m| m.hijo == hijo && (padre.is_none() || m.padre == padre))
        {
            m.lineas.extend(lineas);
            return;
        }
        self.mediciones.push(MedicionBc3 {
            padre,
            hijo,
            total,
            lineas,
            linea,
        });
    }

    // -------------------------------------------------------------- construcción

    fn construir(mut self) -> Result<Importacion, ErrorBc3> {
        if self.conceptos.is_empty() {
            return Err(ErrorBc3::SinConceptos);
        }
        let decimales = Decimales::default();

        // 1. Conceptos referenciados que no existen → marcador con precio 0.
        let mut faltan: BTreeMap<String, usize> = BTreeMap::new();
        for (padre, lineas) in &self.desc {
            if !self.conceptos.contains_key(padre) {
                faltan
                    .entry(padre.clone())
                    .or_insert(lineas.first().map_or(0, |l| l.linea));
            }
            for l in lineas {
                if !self.conceptos.contains_key(&l.hijo) {
                    faltan.entry(l.hijo.clone()).or_insert(l.linea);
                }
            }
        }
        for (codigo, linea) in faltan {
            self.anotar(
                Gravedad::Error,
                linea,
                "D",
                format!("«{codigo}» se usa en una descomposición pero no está definido (~C): se crea con precio 0"),
            );
            self.orden.push(codigo.clone());
            self.conceptos.insert(
                codigo.clone(),
                ConceptoBc3 {
                    codigo,
                    unidad: String::new(),
                    resumen: "(no definido en el BC3)".into(),
                    precio: None,
                    tipo: String::new(),
                    capitulo: false,
                    raiz: false,
                },
            );
        }

        // 2. Naturaleza y conceptos del modelo.
        let mut conceptos: BTreeMap<String, Concepto> = BTreeMap::new();
        let mut es_pct: BTreeSet<String> = BTreeSet::new();
        let mut avisos = Vec::new();
        for codigo in &self.orden {
            let c = &self.conceptos[codigo];
            let tiene_desc = self.desc.get(codigo).is_some_and(|v| !v.is_empty());
            let naturaleza = if c.capitulo {
                Naturaleza::Capitulo
            } else if c.codigo.contains(['%', '&']) {
                es_pct.insert(codigo.clone());
                Naturaleza::Porcentaje
            } else if tiene_desc {
                if matches!(c.tipo.as_str(), "1" | "2" | "3") {
                    avisos.push(format!(
                        "{codigo}: tipo {} con descomposición; se trata como precio descompuesto",
                        c.tipo
                    ));
                }
                Naturaleza::Partida
            } else {
                match c.tipo.as_str() {
                    "1" => Naturaleza::ManoObra,
                    "2" => Naturaleza::Maquinaria,
                    "3" => Naturaleza::Material,
                    _ => Naturaleza::Otros,
                }
            };
            let precio = if naturaleza.es_basico() {
                c.precio.unwrap_or_default()
            } else {
                Decimal::ZERO
            };
            conceptos.insert(
                codigo.clone(),
                Concepto {
                    codigo: codigo.clone(),
                    unidad: c.unidad.clone(),
                    resumen: c.resumen.clone(),
                    naturaleza,
                    precio,
                    descomposicion: Vec::new(),
                    texto: self.textos.remove(codigo),
                },
            );
        }
        for a in avisos {
            self.anotar(Gravedad::Aviso, 0, "C", a);
        }
        for codigo in self.textos.keys().cloned().collect::<Vec<_>>() {
            self.anotar(
                Gravedad::Aviso,
                0,
                "T",
                format!("texto de «{codigo}», que no existe: se ignora"),
            );
        }

        // 3. Mediciones: se asocian a su (padre, hijo).
        let padres_de: HashMap<String, Vec<String>> = {
            let mut m: HashMap<String, Vec<String>> = HashMap::new();
            for (p, ls) in &self.desc {
                for l in ls {
                    m.entry(l.hijo.clone()).or_default().push(p.clone());
                }
            }
            m
        };
        let mut mediciones: BTreeMap<(String, String), (Medicion, Option<Decimal>, usize)> = BTreeMap::new();
        for m in std::mem::take(&mut self.mediciones) {
            let padre = match m.padre {
                Some(p) => Some(p),
                None => match padres_de.get(&m.hijo).map(Vec::as_slice) {
                    Some([unico]) => Some(unico.clone()),
                    Some(varios) if varios.len() > 1 => {
                        self.anotar(
                            Gravedad::Error,
                            m.linea,
                            "M",
                            format!(
                                "{}: medición sin capítulo y la partida está en varios; se ignora",
                                m.hijo
                            ),
                        );
                        None
                    }
                    _ => None,
                },
            };
            let Some(padre) = padre else {
                if padres_de.get(&m.hijo).is_none_or(|v| v.len() <= 1) {
                    self.anotar(
                        Gravedad::Error,
                        m.linea,
                        "M",
                        format!(
                            "{}: medición de una partida que no está en ningún capítulo; se ignora",
                            m.hijo
                        ),
                    );
                }
                continue;
            };
            let existe = self
                .desc
                .get(&padre)
                .is_some_and(|ls| ls.iter().any(|l| l.hijo == m.hijo));
            if !existe {
                self.anotar(
                    Gravedad::Error,
                    m.linea,
                    "M",
                    format!(
                        "{padre} → {}: la medición no corresponde a ninguna línea de descomposición; se ignora",
                        m.hijo
                    ),
                );
                continue;
            }
            mediciones.insert((padre, m.hijo), (Medicion::new(m.lineas), m.total, m.linea));
        }

        // 4. Descomposiciones.
        let mut info_pct = false;
        for (padre, lineas) in std::mem::take(&mut self.desc) {
            let es_cap = conceptos
                .get(&padre)
                .is_some_and(|c| c.naturaleza == Naturaleza::Capitulo);
            let mut out = Vec::with_capacity(lineas.len());
            for l in lineas {
                let factor = l.factor.unwrap_or(Decimal::ONE);
                let rendimiento = match l.rendimiento {
                    Some(r) => r,
                    None => {
                        let m = mediciones.get(&(padre.clone(), l.hijo.clone()));
                        match m.and_then(|(_, total, _)| *total) {
                            Some(t) => t,
                            None => {
                                if !es_cap {
                                    self.anotar(
                                        Gravedad::Aviso,
                                        l.linea,
                                        "D",
                                        format!("{padre} → {}: rendimiento vacío, se toma 1", l.hijo),
                                    );
                                }
                                Decimal::ONE
                            }
                        }
                    }
                };
                let linea = if es_pct.contains(&l.hijo) {
                    // FIEBDC-3: el rendimiento de un porcentaje es una fracción
                    // (0.03 = 3 %). El motor trabaja en puntos.
                    info_pct = true;
                    let puntos = (factor * l.rendimiento.unwrap_or(Decimal::ZERO) * Decimal::ONE_HUNDRED).normalize();
                    if puntos.abs() > Decimal::ONE_HUNDRED {
                        self.anotar(
                            Gravedad::Aviso,
                            l.linea,
                            "D",
                            format!(
                                "{padre} → {}: porcentaje del {} %; ¿el rendimiento venía en puntos en vez de en fracción?",
                                l.hijo,
                                num(puntos, 2)
                            ),
                        );
                    }
                    LineaDescomposicion {
                        hijo: l.hijo,
                        factor: Decimal::ONE,
                        rendimiento: puntos,
                    }
                } else {
                    LineaDescomposicion {
                        hijo: l.hijo,
                        factor,
                        rendimiento,
                    }
                };
                out.push(linea);
            }
            if let Some(c) = conceptos.get_mut(&padre) {
                c.descomposicion = out;
            }
        }
        if info_pct {
            self.anotar(
                Gravedad::Info,
                0,
                "D",
                "líneas porcentuales (código con «%» o «&»): el rendimiento es una fracción (0.03 = 3 %) y se aplica \
                 a las líneas anteriores cuyo código empieza por la máscara (prefijo antes del «%»). \
                 Conforme a FIEBDC-3; pendiente de contrastar con Presto 8.8 (P-012).",
            );
        }

        // 5. Comprobación de mediciones frente a la cantidad del ~D.
        for ((padre, hijo), (m, total_decl, linea)) in &mediciones {
            let unidad = conceptos.get(hijo).map(|c| c.unidad.clone()).unwrap_or_default();
            let cantidad_d = conceptos
                .get(padre)
                .and_then(|c| c.descomposicion.iter().find(|l| &l.hijo == hijo))
                .map(|l| redondear(l.cantidad(), decimales.medicion));
            match m.calcular(&decimales, &unidad) {
                Ok(r) => {
                    let referencia = cantidad_d.or(*total_decl);
                    if let Some(q) = referencia
                        && r.total != q
                    {
                        self.anotar(
                            Gravedad::Aviso,
                            *linea,
                            "M",
                            format!(
                                "{padre} → {hijo}: las líneas de medición suman {} pero la cantidad es {}; se mantiene la cantidad",
                                num(r.total, decimales.medicion),
                                num(q, decimales.medicion)
                            ),
                        );
                    }
                }
                Err(e) => self.anotar(Gravedad::Error, *linea, "M", format!("{padre} → {hijo}: {e}")),
            }
        }

        // 6. Raíz.
        let raices: Vec<String> = self.orden.iter().filter(|c| self.conceptos[*c].raiz).cloned().collect();
        let raiz = match raices.as_slice() {
            [r] => r.clone(),
            [r, ..] => {
                self.anotar(
                    Gravedad::Aviso,
                    0,
                    "C",
                    format!("hay {} conceptos raíz (##); se toma «{r}»", raices.len()),
                );
                r.clone()
            }
            [] => {
                let hijos: BTreeSet<&String> = conceptos
                    .values()
                    .flat_map(|c| c.descomposicion.iter().map(|l| &l.hijo))
                    .collect();
                let sueltos: Vec<String> = self
                    .orden
                    .iter()
                    .filter(|c| !hijos.contains(c) && conceptos[*c].naturaleza == Naturaleza::Capitulo)
                    .cloned()
                    .collect();
                if let [unico] = sueltos.as_slice() {
                    self.anotar(
                        Gravedad::Aviso,
                        0,
                        "C",
                        format!("sin concepto raíz (##): se toma el capítulo «{unico}»"),
                    );
                    unico.clone()
                } else {
                    let mut codigo = RAIZ_SINTETICA.to_owned();
                    while conceptos.contains_key(&codigo) {
                        codigo.push('_');
                    }
                    self.anotar(
                        Gravedad::Aviso,
                        0,
                        "C",
                        format!(
                            "sin concepto raíz (##): se crea «{codigo}» con {} capítulos sueltos",
                            sueltos.len()
                        ),
                    );
                    let mut r = Concepto::capitulo(codigo.clone(), self.cabecera.rotulo.clone());
                    r.descomposicion = sueltos
                        .iter()
                        .map(|c| LineaDescomposicion::new(c.clone(), Decimal::ONE))
                        .collect();
                    conceptos.insert(codigo.clone(), r);
                    codigo
                }
            }
        };
        if let Some(c) = conceptos.get_mut(&raiz)
            && c.naturaleza != Naturaleza::Capitulo
        {
            c.naturaleza = Naturaleza::Capitulo;
        }

        let nombre = conceptos
            .get(&raiz)
            .map(|c| c.resumen.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if self.cabecera.rotulo.is_empty() {
                    "Presupuesto importado".into()
                } else {
                    self.cabecera.rotulo.clone()
                }
            });
        let mut p = Presupuesto {
            nombre,
            decimales,
            raiz: raiz.clone(),
            conceptos,
            mediciones: mediciones.into_iter().map(|(k, (m, _, _))| (k, m)).collect(),
            costes_indirectos: self.porcentajes.costes_indirectos.unwrap_or_default(),
            opciones_ci: OpcionesCi::default(),
        };

        // 7. Ciclos: se rompe la última relación de cada ciclo encontrado.
        for _ in 0..1000 {
            match p.validar() {
                Ok(()) => break,
                Err(ErrorMotor::ReferenciaCircular(ciclo)) if ciclo.len() >= 2 => {
                    let (a, b) = (&ciclo[ciclo.len() - 2], &ciclo[ciclo.len() - 1]);
                    self.anotar(
                        Gravedad::Error,
                        0,
                        "D",
                        format!(
                            "referencia circular {}: se elimina «{b}» de la descomposición de «{a}»",
                            ciclo.join(" → ")
                        ),
                    );
                    if let Some(c) = p.conceptos.get_mut(a) {
                        c.descomposicion.retain(|l| &l.hijo != b);
                    }
                    p.mediciones.remove(&(a.clone(), b.clone()));
                }
                Err(e) => {
                    self.anotar(Gravedad::Error, 0, "", format!("presupuesto inconsistente: {e}"));
                    break;
                }
            }
        }

        // 8. Registros no interpretados.
        for (t, nombre) in [
            ("L", "pliegos"),
            ("Q", "pliegos"),
            ("J", "pliegos"),
            ("P", "descripciones paramétricas"),
            ("W", "ámbitos geográficos"),
            ("G", "información gráfica"),
            ("E", "entidades"),
            ("O", "relaciones comerciales"),
            ("X", "información técnica"),
            ("A", "claves de tesauro"),
            ("F", "documentos adjuntos"),
            ("R", "residuos"),
            ("B", "cambios de código"),
        ] {
            if let Some(n) = self.registros.get(t).copied() {
                self.anotar(
                    Gravedad::Info,
                    0,
                    t,
                    format!("{n} registro(s) ~{t} ({nombre}) no se importan en esta versión"),
                );
            }
        }

        // 9. Recalcular y comparar con los precios declarados.
        let mut discrepancias = Vec::new();
        let mut pem_declarado = None;
        match p.valorar() {
            Ok(v) => {
                // Precio con costes indirectos de cada concepto colgado de un capítulo
                let venta: HashMap<&str, Decimal> = p
                    .conceptos
                    .values()
                    .filter(|c| c.naturaleza == Naturaleza::Capitulo)
                    .filter_map(|c| v.lineas.get(&c.codigo))
                    .flatten()
                    .map(|l| (l.hijo.as_str(), l.precio))
                    .collect();
                for (codigo, c) in &p.conceptos {
                    if c.descomposicion.is_empty() || c.naturaleza == Naturaleza::Porcentaje {
                        continue;
                    }
                    let Some(declarado) = self.conceptos.get(codigo).and_then(|b| b.precio) else {
                        continue;
                    };
                    let dec = if c.naturaleza == Naturaleza::Capitulo {
                        decimales.importe
                    } else {
                        decimales.precio
                    };
                    let declarado = redondear(declarado, dec);
                    if codigo == &raiz {
                        pem_declarado = Some(declarado);
                    }
                    let calculado = match venta.get(codigo.as_str()) {
                        Some(pv) if c.naturaleza != Naturaleza::Capitulo => *pv,
                        _ => v.precios.get(codigo).copied().unwrap_or_default(),
                    };
                    if calculado != declarado {
                        discrepancias.push(Discrepancia {
                            codigo: codigo.clone(),
                            resumen: c.resumen.clone(),
                            declarado,
                            calculado,
                        });
                    }
                }
            }
            Err(e) => self.anotar(
                Gravedad::Error,
                0,
                "",
                format!("no se pudo valorar el presupuesto: {e}"),
            ),
        }
        discrepancias.sort_by(|a, b| {
            b.diferencia()
                .abs()
                .cmp(&a.diferencia().abs())
                .then(a.codigo.cmp(&b.codigo))
        });
        self.incidencias
            .sort_by_key(|i| (std::cmp::Reverse(i.gravedad), i.linea));

        Ok(Importacion {
            presupuesto: p,
            cabecera: self.cabecera,
            porcentajes: self.porcentajes,
            registros: self.registros,
            incidencias: self.incidencias,
            discrepancias,
            pem_declarado,
        })
    }
}

// ---------------------------------------------------------------------------

/// Quita espacios y las almohadillas finales que marcan capítulos y raíz.
fn normalizar(codigo: &str) -> String {
    codigo.trim().trim_end_matches('#').trim().to_owned()
}

/// Número BC3 (punto decimal). Vacío → `None`. Admite coma decimal por tolerancia.
fn numero(s: &str) -> Result<Option<Decimal>, ()> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(None);
    }
    let t = if t.contains(',') && !t.contains('.') {
        t.replace(',', ".")
    } else {
        t.to_owned()
    };
    Decimal::from_str(&t)
        .or_else(|_| Decimal::from_scientific(&t))
        .map(Some)
        .map_err(|_| ())
}

/// Sexto campo del primer `~V` (juego de caracteres), leído byte a byte.
fn juego_declarado(bytes: &[u8]) -> String {
    let Some(ini) = bytes.windows(2).position(|w| w == b"~V" || w == b"~v") else {
        return String::new();
    };
    let fin = bytes[ini + 1..]
        .iter()
        .position(|&b| b == b'~')
        .map_or(bytes.len(), |p| ini + 1 + p);
    bytes[ini..fin]
        .split(|&b| b == b'|')
        .nth(5)
        .map(|c| String::from_utf8_lossy(c).trim().to_owned())
        .unwrap_or_default()
}

fn win1252(bytes: &[u8]) -> String {
    encoding_rs::WINDOWS_1252
        .decode_without_bom_handling(bytes)
        .0
        .into_owned()
}
