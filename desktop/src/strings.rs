// SPDX-License-Identifier: Apache-2.0

//! Words that belong to this shell: button titles, headings, short chips.
//!
//! Every sentence about a scan comes from the core as a `Line` and is shown with
//! [`say`]. Nothing here is compared to decide state. Each label has a stable id,
//! so a gettext catalog can replace the English table later without touching callers.

use openworld_core::Line;

/// A label id. The English is in [`text`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum S {
    AppName,
    Tagline,
    ChooseFile,
    OrDropHere,
    DropRelease,
    TrustOnDevice,
    TrustNothingUploaded,
    TrustNoCamera,
    NoPosters,
    InstallDemo,
    UpdatePosters,
    DemoBadge,
    ResultsKept,
    Review,
    Photo,
    Video,
    Estimating,
    CompareWith,
    Missing,
    Wanted,
    Options,
    ModelBundle,
    Coverage,
    EveryFrame,
    FiveASecond,
    WhatHappens,
    ReadAll,
    Disclosure,
    Analyze,
    ChooseAnother,
    Scanning,
    ScanningOnDevice,
    Faces,
    NoFacesYet,
    Stop,
    Stopping,
    StopTitle,
    StopBody,
    KeepScanning,
    Result,
    Candidates,
    ShowFrame,
    OpenFbi,
    NoFbiPage,
    Stay,
    Details,
    OtherFaces,
    DeleteResult,
    DeleteTitle,
    DeleteBody,
    Delete,
    Cancel,
    ScanAnother,
    Close,
    ChipLooking,
    ChipCompared,
    ChipNotCompared,
    ChipCandidate,
    ChipTooBrief,
    ChipUnscored,
    FileFilterMedia,
    FileFilterAll,
    Unofficial,
    DetailFrames,
    DetailCoverage,
    DetailDetection,
    DetailChance,
    DetailClasses,
    DetailBundle,
    DetailPosters,
    DetailDevice,
    DetailPerception,
    DetailWarnings,
    FileFrom,
    HintChooseFile,
    HintInstallDemo,
    HintCheckClock,
    HintInstallWeights,
    HintTryAgain,
    YourFile,
    Poster,
    Unopened,
    About,
    MainMenu,
    Welcome,
    Continue,
    Publisher,
}

/// True when the environment asks for Spanish (`LC_ALL`/`LC_MESSAGES`/`LANG` begins
/// with `es`). Read once. Anything else, including unset, is English.
fn is_spanish() -> bool {
    // Tests assert the authoritative English copy, so they must not depend on the
    // runner's locale; the es draft is covered by its own review gate (docs/localization.md).
    #[cfg(test)]
    {
        false
    }
    #[cfg(not(test))]
    {
        static SPANISH: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *SPANISH.get_or_init(|| {
            for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
                if let Ok(value) = std::env::var(key) {
                    if !value.is_empty() {
                        return value.to_ascii_lowercase().starts_with("es");
                    }
                }
            }
            false
        })
    }
}

/// A shell label. English is authoritative; a Spanish locale gets the draft es
/// translation ([`text_es`]) and falls back to English for anything untranslated.
pub fn text(id: S) -> &'static str {
    if is_spanish() {
        if let Some(es) = text_es(id) {
            return es;
        }
    }
    text_en(id)
}

/// The English label. The source of truth for every shell string.
fn text_en(id: S) -> &'static str {
    match id {
        S::AppName => "OpenWorld",
        S::Tagline => "Check a photo or video you already have against public FBI missing and wanted posters, on this device.",
        S::ChooseFile => "Choose a photo or video",
        S::OrDropHere => "or drop it here",
        S::DropRelease => "Release to open this file",
        S::TrustOnDevice => "Stays on this device",
        S::TrustNothingUploaded => "Nothing uploaded",
        S::TrustNoCamera => "No camera",
        S::NoPosters => "No posters installed. Real FBI photos stay off.",
        S::InstallDemo => "Install demo posters",
        S::UpdatePosters => "Update posters",
        S::DemoBadge => "Demo",
        S::ResultsKept => "Results are deleted after 7 days.",
        S::Review => "Review",
        S::Photo => "Photo",
        S::Video => "Video",
        S::Estimating => "Estimating the time…",
        S::CompareWith => "Compare with",
        S::Missing => "Missing",
        S::Wanted => "Wanted",
        S::Options => "Options",
        S::ModelBundle => "Model bundle",
        S::Coverage => "Coverage",
        S::EveryFrame => "Every frame",
        S::FiveASecond => "5 frames a second",
        S::WhatHappens => "What happens to this file",
        S::ReadAll => "Read all",
        S::Disclosure => "What OpenWorld does and does not do",
        S::Analyze => "Analyze on this device",
        S::ChooseAnother => "Choose another file",
        S::Scanning => "Scanning",
        S::ScanningOnDevice => "Scanning on this device",
        S::Faces => "Faces",
        S::NoFacesYet => "Faces appear here as they are found.",
        S::Stop => "Stop",
        S::Stopping => "Stopping…",
        S::StopTitle => "Stop the scan?",
        S::StopBody => "A stopped scan is Incomplete. It is never a clearance.",
        S::KeepScanning => "Keep scanning",
        S::Result => "Result",
        S::Candidates => "Possible candidates",
        S::ShowFrame => "Show frame",
        S::OpenFbi => "Open FBI page",
        S::NoFbiPage => "This poster has no FBI page.",
        S::Stay => "Stay",
        S::Details => "Details",
        S::OtherFaces => "Other faces",
        S::DeleteResult => "Delete result",
        S::DeleteTitle => "Delete this result?",
        S::DeleteBody => "The crops, frames, and result file are removed from this device. Your original file is not touched.",
        S::Delete => "Delete",
        S::Cancel => "Cancel",
        S::ScanAnother => "Scan another file",
        S::Close => "Close",
        S::ChipLooking => "Looking",
        S::ChipCompared => "Compared",
        S::ChipNotCompared => "Not compared",
        S::ChipCandidate => "Possible candidate",
        S::ChipTooBrief => "Too brief",
        S::ChipUnscored => "Not scored",
        S::FileFilterMedia => "Photos and videos",
        S::FileFilterAll => "All files",
        S::Unofficial => "Unofficial",
        S::DetailFrames => "Frames",
        S::DetailCoverage => "Coverage",
        S::DetailDetection => "Detection size",
        S::DetailChance => "Chance",
        S::DetailClasses => "Classes",
        S::DetailBundle => "Model bundle",
        S::DetailPosters => "Poster pack",
        S::DetailDevice => "Ran on",
        S::DetailPerception => "Posters",
        S::DetailWarnings => "File",
        S::FileFrom => "From your file",
        S::HintChooseFile => "Choose another photo or video.",
        S::HintInstallDemo => "Go back to the start screen to install demo posters.",
        S::HintCheckClock => "Check this computer's date and time, then try again.",
        S::HintInstallWeights => "Install the bundle weights (scripts/fetch_weights.py), or use the demo posters.",
        S::HintTryAgain => "Try again, or choose another file.",
        S::YourFile => "Your file",
        S::Poster => "Poster",
        S::Unopened => "The page could not be opened.",
        S::About => "About OpenWorld",
        S::MainMenu => "Main menu",
        S::Welcome => "Welcome to OpenWorld",
        S::Continue => "Continue",
        S::Publisher => "OneX Engineering",
    }
}

/// The draft Spanish label, or `None` to fall back to English. UNREVIEWED machine
/// translation pending professional legal/safety review; English governs. Brand and
/// company names (OpenWorld, OneX Engineering, the Demo badge) are kept verbatim.
fn text_es(id: S) -> Option<&'static str> {
    Some(match id {
        S::AppName | S::DemoBadge | S::Publisher => return None,
        S::Tagline => "Compara una foto o un video que ya tienes con carteles públicos del FBI de personas desaparecidas y buscadas, en este dispositivo.",
        S::ChooseFile => "Elegir una foto o un video",
        S::OrDropHere => "o suéltalo aquí",
        S::DropRelease => "Suelta para abrir este archivo",
        S::TrustOnDevice => "Permanece en este dispositivo",
        S::TrustNothingUploaded => "No se sube nada",
        S::TrustNoCamera => "Sin cámara",
        S::NoPosters => "No hay carteles instalados. Las fotos reales del FBI permanecen desactivadas.",
        S::InstallDemo => "Instalar carteles de demostración",
        S::UpdatePosters => "Actualizar carteles",
        S::ResultsKept => "Los resultados se eliminan después de 7 días.",
        S::Review => "Revisar",
        S::Photo => "Foto",
        S::Video => "Video",
        S::Estimating => "Estimando el tiempo…",
        S::CompareWith => "Comparar con",
        S::Missing => "Desaparecidos",
        S::Wanted => "Buscados",
        S::Options => "Opciones",
        S::ModelBundle => "Paquete de modelos",
        S::Coverage => "Cobertura",
        S::EveryFrame => "Cada fotograma",
        S::FiveASecond => "5 fotogramas por segundo",
        S::WhatHappens => "Qué le pasa a este archivo",
        S::ReadAll => "Leer todo",
        S::Disclosure => "Lo que OpenWorld hace y no hace",
        S::Analyze => "Analizar en este dispositivo",
        S::ChooseAnother => "Elegir otro archivo",
        S::Scanning => "Analizando",
        S::ScanningOnDevice => "Analizando en este dispositivo",
        S::Faces => "Rostros",
        S::NoFacesYet => "Los rostros aparecen aquí a medida que se encuentran.",
        S::Stop => "Detener",
        S::Stopping => "Deteniendo…",
        S::StopTitle => "¿Detener el análisis?",
        S::StopBody => "Un análisis detenido queda Incompleto. Nunca es una exoneración.",
        S::KeepScanning => "Seguir analizando",
        S::Result => "Resultado",
        S::Candidates => "Posibles candidatos",
        S::ShowFrame => "Mostrar fotograma",
        S::OpenFbi => "Abrir página del FBI",
        S::NoFbiPage => "Este cartel no tiene página del FBI.",
        S::Stay => "Quedarse",
        S::Details => "Detalles",
        S::OtherFaces => "Otros rostros",
        S::DeleteResult => "Eliminar resultado",
        S::DeleteTitle => "¿Eliminar este resultado?",
        S::DeleteBody => "Los recortes, los fotogramas y el archivo de resultado se eliminan de este dispositivo. Tu archivo original no se toca.",
        S::Delete => "Eliminar",
        S::Cancel => "Cancelar",
        S::ScanAnother => "Analizar otro archivo",
        S::Close => "Cerrar",
        S::ChipLooking => "Buscando",
        S::ChipCompared => "Comparado",
        S::ChipNotCompared => "No comparado",
        S::ChipCandidate => "Posible candidato",
        S::ChipTooBrief => "Demasiado breve",
        S::ChipUnscored => "Sin puntuar",
        S::FileFilterMedia => "Fotos y videos",
        S::FileFilterAll => "Todos los archivos",
        S::Unofficial => "No oficial",
        S::DetailFrames => "Fotogramas",
        S::DetailCoverage => "Cobertura",
        S::DetailDetection => "Tamaño de detección",
        S::DetailChance => "Probabilidad",
        S::DetailClasses => "Clases",
        S::DetailBundle => "Paquete de modelos",
        S::DetailPosters => "Paquete de carteles",
        S::DetailDevice => "Se ejecutó en",
        S::DetailPerception => "Carteles",
        S::DetailWarnings => "Archivo",
        S::FileFrom => "De tu archivo",
        S::HintChooseFile => "Elige otra foto o video.",
        S::HintInstallDemo => "Vuelve a la pantalla de inicio para instalar los carteles de demostración.",
        S::HintCheckClock => "Revisa la fecha y la hora de esta computadora y vuelve a intentarlo.",
        S::HintInstallWeights => "Instala los pesos del paquete (scripts/fetch_weights.py) o usa los carteles de demostración.",
        S::HintTryAgain => "Inténtalo de nuevo o elige otro archivo.",
        S::YourFile => "Tu archivo",
        S::Poster => "Cartel",
        S::Unopened => "No se pudo abrir la página.",
        S::About => "Acerca de OpenWorld",
        S::MainMenu => "Menú principal",
        S::Welcome => "Te damos la bienvenida a OpenWorld",
        S::Continue => "Continuar",
    })
}

/// A sentence from the core, translated by `line.code`. English is authoritative.
/// A Spanish locale gets the draft es translation of the fixed `copy` sentences;
/// anything else, and any line that carries `args` (its English text already holds the
/// filled-in values), falls back to the core's own text.
pub fn say(line: &Line) -> &str {
    if is_spanish() && line.args.is_empty() {
        if let Some(es) = core_es(&line.code) {
            return es;
        }
    }
    &line.text
}

/// Draft Spanish for the core's fixed `copy` sentences, by code. UNREVIEWED machine
/// translation pending professional legal/safety review; English governs. FBI, NCMEC,
/// OpenWorld, 911, phone numbers and URLs are kept verbatim.
fn core_es(code: &str) -> Option<&'static str> {
    Some(match code {
        "accept_terms" => "Al continuar, aceptas los Términos de uso, incluido el arbitraje individual vinculante y la renuncia a demandas colectivas, y la Política de privacidad y la Política de datos biométricos.",
        "adults_us" => "Para adultos en los Estados Unidos.",
        "already_warm" => "Este teléfono ya está caliente. El análisis podría detenerse antes de tiempo.",
        "bad_codec" => "Códec incorrecto o archivo ilegible. Se rechaza.",
        "bad_hash" => "El hash del paquete de carteles no coincide. Se rechaza.",
        "bad_request" => "No se entendió la solicitud de análisis. Se rechaza.",
        "bad_vectors" => "Los vectores del paquete de carteles no son válidos para este paquete de modelos. Se rechaza.",
        "band_above" => "Claramente por encima del umbral fijado.",
        "band_near" => "Apenas por encima del umbral fijado.",
        "band_well" => "Muy por encima del umbral fijado.",
        "battery_note" => "Un análisis largo consume mucha batería.",
        "below_cutoff" => "Por debajo del umbral fijado. No es un candidato.",
        "brief_face" => "Un rostro que aparece brevemente puede pasar desapercibido.",
        "bundle_not_found" => "Ese paquete de modelos no está en el catálogo. Se rechaza.",
        "bundle_note" => "Las puntuaciones no son comparables entre paquetes de modelos. Los resultados indican el paquete que elijas.",
        "cancelled" => "Detuviste el análisis antes de que se analizaran todos los fotogramas.",
        "catalog" => "No se pudo leer el catálogo de paquetes de modelos. Se rechaza.",
        "chance_unknown" => "Aún no se ha medido con qué frecuencia una persona desconocida supera este umbral.",
        "choose_class" => "Elige desaparecidos, buscados o ambos.",
        "classes_both" => "Desaparecidos y buscados.",
        "classes_missing" => "Desaparecidos.",
        "classes_wanted" => "Buscados.",
        "coverage_complete" => "Todos los fotogramas decodificados.",
        "coverage_measured" => "5 fotogramas por segundo, más el seguimiento.",
        "cpu_note" => "Este análisis se ejecuta en la CPU. Será más lento, generará más calor y consumirá más batería.",
        "delete_failed" => "No se pudo eliminar el resultado.",
        "delete_foreign" => "Se rechaza eliminar un directorio que no es un resultado de OpenWorld.",
        "deleted" => "Eliminado.",
        "do_not_approach" => "No te acerques, no sigas, no te enfrentes ni intentes retener a nadie. Si alguien corre peligro inmediato, llama al 911.",
        "do_not_share" => "No publiques ni compartas este resultado. Un posible candidato puede ser una persona desconocida que se parece.",
        "doc_biometric" => "Política de datos biométricos",
        "doc_health_data" => "Política de privacidad de datos de salud del consumidor",
        "doc_licenses" => "Licencias",
        "doc_privacy" => "Política de privacidad",
        "doc_terms" => "Términos de uso",
        "embedder_mismatch" => "El paquete de carteles se creó con otro codificador. Se rechaza.",
        "estimate_caveat" => "Esta es una estimación de planificación, no una medición térmica.",
        "estimate_rough" => "No se pudo cronometrar el modelo en este dispositivo, así que esta estimación es aproximada.",
        "expired_pack" => "El paquete de carteles ha caducado. Se rechaza.",
        "face_unscored" => "No se pudo puntuar este rostro.",
        "fixture_markers" => "Carteles de demostración. Son marcadores sintéticos, no rostros.",
        "fixture_off" => "Esta versión no lee carteles de demostración. Se rechaza.",
        "future_pack" => "El paquete de carteles tiene una fecha futura. Revisa el reloj de este dispositivo. Se rechaza.",
        "gpu_note" => "Este análisis se ejecuta en la GPU y puede calentar el dispositivo.",
        "groups_unmeasured" => "Aún no se ha medido con qué frecuencia esto identifica erróneamente a personas de distintas edades, sexos y tonos de piel.",
        "heat_note" => "Este teléfono puede calentarse. Si el calor o el sistema detienen el análisis, el resultado queda Incompleto.",
        "incomplete" => "Incompleto.",
        "internal" => "La biblioteca de análisis falló. Se rechaza.",
        "lawful_use" => "Analiza solo archivos que tengas derecho a usar. Algunos lugares restringen el análisis de los rostros de otras personas.",
        "leave_missing" => "Esto abre un cartel de persona desaparecida del FBI. No es una acusación penal. No te acerques, no sigas ni intentes retener a nadie, en especial a un menor. Los carteles de menores desaparecidos suelen indicar NCMEC en el 1-800-THE-LOST (1-800-843-5678).",
        "leave_wanted" => "Esto abre un cartel de persona buscada del FBI. No te acerques, no sigas ni te enfrentes a nadie. Si alguien corre peligro inmediato, llama al 911. Los carteles de personas buscadas suelen indicar el 1-800-CALL-FBI (1-800-225-5324) o tips.fbi.gov.",
        "leaving" => "Estás saliendo de OpenWorld.",
        "lookalike" => "Personas parecidas, familiares y fotos antiguas pueden superar el umbral.",
        "missing_pack" => "Falta el paquete de carteles. Se rechaza.",
        "missing_program" => "El programa de análisis no está en este dispositivo. Se rechaza.",
        "missing_weights" => "Los pesos del paquete de modelos no están instalados, o su SHA-256 no está fijado. Se rechaza.",
        "motion_clip_unread" => "No se analizó el video de esta foto en movimiento.",
        "network_note" => "Abrir un cartel visita fbi.gov, y actualizar los carteles los descarga de api.fbi.gov. Esos sitios ven la dirección de internet de tu dispositivo, como cualquier sitio web.",
        "network_off" => "Esta versión no tiene código de red. Los carteles no se cambiaron.",
        "no_agency" => "OpenWorld no informa nada a ninguna agencia.",
        "no_camera" => "Importa un archivo que ya tengas. No hay cámara.",
        "no_class" => "No había ninguna clase activada.",
        "no_clearance" => "La ausencia de candidatos no es una exoneración.",
        "no_document" => "Ese documento no está en esta versión. Se rechaza.",
        "no_fps" => "El decodificador no informó una velocidad de fotogramas. Se rechaza.",
        "no_posters_in_class" => "El paquete de carteles no tiene carteles de las clases que elegiste. Se rechaza.",
        "no_training" => "OpenWorld no se entrena con este archivo.",
        "nobody_enrolled" => "No hay nadie inscrito.",
        "not_affiliated" => "OpenWorld no forma parte del FBI ni de ninguna agencia gubernamental, ni cuenta con su respaldo.",
        "not_compared" => "No comparado.",
        "not_fully_decoded" => "El archivo no se decodificó por completo.",
        "not_identification" => "Un candidato no es una identificación.",
        "not_measured" => "Aún no medido.",
        "not_real_proof" => "Que sea en el dispositivo no significa que el archivo sea real.",
        "not_sending" => "OpenWorld no está enviando ningún informe.",
        "nothing_uploaded" => "No se sube nada.",
        "old_file" => "Este archivo tiene más de unos 30 días.",
        "old_pack" => "Los carteles tienen más de 7 días. Actualiza los carteles. Se rechaza.",
        "on_device" => "Este archivo permanece en este dispositivo.",
        "only_fbi" => "OpenWorld solo abre una página del FBI.",
        "out_create" => "No se pudo crear el directorio de salida. Se rechaza.",
        "out_foreign" => "Esa carpeta no está vacía y no es un resultado de OpenWorld. Elige una carpeta vacía. Se rechaza.",
        "out_overlap" => "La carpeta de resultados no puede contener el archivo, los carteles ni los paquetes de modelos. Se rechaza.",
        "out_replace" => "No se pudo reemplazar el directorio de salida. Se rechaza.",
        "possible_candidate" => "Posible candidato. No es una identificación.",
        "posters_bad_list" => "La lista de carteles del FBI no tenía el formato esperado. Los carteles no se cambiaron.",
        "posters_fetch_failed" => "No se pudo obtener la lista de carteles del FBI. Los carteles no se cambiaron.",
        "posters_foreign" => "Esa carpeta no está vacía y no es un paquete de carteles. Elige una carpeta vacía. Se rechaza.",
        "posters_none" => "Ninguna foto de cartel superó las comprobaciones. Los carteles no se cambiaron.",
        "posters_runtime" => "Los modelos de rostros se detuvieron al comprobar las fotos de los carteles. Los carteles no se cambiaron.",
        "posters_stale" => "Los carteles se obtuvieron hace más de un día.",
        "posters_unwritten" => "No se pudieron escribir los nuevos carteles. Los carteles no se cambiaron.",
        "real_posters_off" => "Las fotos reales del FBI permanecen desactivadas. Ningún paquete de modelos tiene una curva que las permita.",
        "result_unwritten" => "No se pudo escribir el resultado.",
        "runtime" => "ONNX Runtime no se cargó. Se rechaza.",
        "scan_killed" => "El sistema cerró OpenWorld antes de que terminara el análisis.",
        "skip_no_face" => "No se encontró ningún rostro en la foto del cartel.",
        "skip_no_photo" => "El cartel no tiene foto en una dirección del FBI.",
        "skip_not_open" => "No es un cartel de persona desaparecida o buscada que esté abierto y con nombre.",
        "skip_not_picture" => "La foto del cartel no es una imagen que OpenWorld pueda leer.",
        "skip_photo_too_large" => "La foto del cartel es demasiado grande para comprobarla de forma segura.",
        "skip_photo_unavailable" => "No se pudo descargar la foto del cartel.",
        "skip_several_faces" => "La foto del cartel muestra más de un rostro.",
        "stopped_background" => "La aplicación salió de la pantalla, así que el análisis se detuvo.",
        "stopped_battery" => "La batería se agotó, así que el análisis se detuvo.",
        "stopped_heat" => "El dispositivo se calentó demasiado, así que el análisis se detuvo.",
        "suggest_computer" => "Una computadora lo terminará antes. Aún puedes ejecutar un análisis completo en este teléfono.",
        "terms_changed" => "Los términos han cambiado. Léelos y luego continúa para seguir usando OpenWorld.",
        "timestamps_disagree" => "Las marcas de tiempo del archivo no coinciden.",
        "too_brief" => "Aparece en pantalla muy poco tiempo para ser un candidato. No es una exoneración.",
        "too_large" => "Esta imagen es demasiado grande para analizarla de forma segura. Se rechaza.",
        "unauthenticated" => "Este archivo no está autenticado.",
        "unknown_perception" => "No se reconoce la percepción del paquete de carteles. Se rechaza.",
        "unreadable" => "No se pudo leer el archivo. Se rechaza.",
        "unreadable_pack" => "No se pudo leer el paquete de carteles. Se rechaza.",
        "welcome" => "OpenWorld compara los rostros de una foto o un video que ya tienes con carteles públicos del FBI de personas desaparecidas y buscadas, en este dispositivo.",
        "why_blur" => "El rostro está demasiado borroso.",
        "why_confidence" => "El detector no estaba seguro de que esto sea un rostro.",
        "why_exposure" => "El rostro está demasiado oscuro o demasiado claro.",
        "why_pose" => "El rostro está girado demasiado respecto a la cámara.",
        "why_small" => "El rostro mide menos de 112 píxeles de ancho en el archivo original.",
        "why_unaligned" => "No se pudieron encontrar los ojos, la nariz y la boca.",
        _ => return None,
    })
}

fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn plural(n: u64, one: &str, many: &str) -> String {
    format!("{} {}", grouped(n), if n == 1 { one } else { many })
}

/// "1,800 frames · faces looked for at 960 px".
pub fn estimate_summary(frames: u64, detection_long: u32, full: bool) -> String {
    let size = if full { "faces looked for at full resolution".to_string() } else { format!("faces looked for at {detection_long} px") };
    format!("{} · {size}", plural(frames, "frame", "frames"))
}

/// "Frame 412 of 1,800".
pub fn progress(analyzed: u64, total: u64) -> String {
    format!("Frame {} of {}", grouped(analyzed.min(total)), grouped(total))
}

/// "About 1 minute left", from the core's duration line.
pub fn time_left(duration: &Line) -> String {
    format!("{} left", say(duration))
}

/// "Demo posters · 2 posters · fetched today".
pub fn pack_footer(demo: bool, posters: usize, days_old: Option<u64>) -> String {
    let kind = if demo { "Demo posters" } else { "FBI posters" };
    let fetched = match days_old {
        Some(0) => "fetched today".to_string(),
        Some(1) => "fetched yesterday".to_string(),
        Some(d) => format!("fetched {d} days ago"),
        None => "fetch date unknown".to_string(),
    };
    format!("{kind} · {} · {fetched}", plural(posters as u64, "poster", "posters"))
}

/// "Reading the FBI list · page 3 of 26", then "Checking poster photos · 120 of 900".
pub fn update_progress(page: u32, pages: Option<u32>, checked: u64, posters: u64) -> String {
    if posters > 0 {
        return format!("Checking poster photos · {} of {}", grouped(checked), grouped(posters));
    }
    match (page, pages) {
        (0, _) => "Reading the FBI list…".to_string(),
        (page, Some(pages)) => format!("Reading the FBI list · page {} of {}", grouped(page.into()), grouped(pages.into())),
        (page, None) => format!("Reading the FBI list · page {}", grouped(page.into())),
    }
}

/// "Kept 812 posters."
pub fn kept(n: u64) -> String {
    format!("Kept {}.", plural(n, "poster", "posters"))
}

/// "12 left out: No face was found on the poster photo."
pub fn skipped(count: u64, reason: &str) -> String {
    format!("{} left out: {reason}", grouped(count))
}

/// A class chip: "Missing · 1".
pub fn class_chip(class: S, count: usize) -> String {
    format!("{} · {}", text(class), grouped(count as u64))
}

/// "1 possible candidate to review".
pub fn candidate_count(n: usize) -> String {
    format!("{} to review.", plural(n as u64, "possible candidate", "possible candidates"))
}

/// The accessible name of a face crop: "Face from frame 12. Possible candidate."
pub fn face_label(frame: u64, status: S) -> String {
    format!("Face from frame {}. {}.", grouped(frame + 1), text(status))
}

/// "Photo · 640 × 480 · 3.1 KB" or "Video · 0:42 · 12.3 MB".
pub fn file_kind(video: bool, width: u32, height: u32, duration_sec: f64, bytes: u64) -> String {
    let size = file_size(bytes);
    if video {
        format!("{} · {} · {size}", text(S::Video), clock(duration_sec))
    } else {
        format!("{} · {width} × {height} · {size}", text(S::Photo))
    }
}

/// "0:42", "1:02:03".
pub fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub fn file_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} bytes");
    }
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// "Recorded 19 Oct 2026" from a date the caller formats for the locale.
pub fn recorded(date: &str) -> String {
    format!("Recorded {date}")
}

/// "Fast 0.2.0".
pub fn bundle_name(name: &str, version: &str) -> String {
    format!("{name} {version}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_naturally() {
        assert_eq!(progress(412, 1800), "Frame 412 of 1,800");
        assert_eq!(estimate_summary(1, 640, true), "1 frame · faces looked for at full resolution");
        assert_eq!(estimate_summary(1800, 960, false), "1,800 frames · faces looked for at 960 px");
        assert_eq!(pack_footer(true, 2, Some(0)), "Demo posters · 2 posters · fetched today");
        assert_eq!(update_progress(0, None, 0, 0), "Reading the FBI list…");
        assert_eq!(update_progress(3, Some(26), 0, 0), "Reading the FBI list · page 3 of 26");
        assert_eq!(update_progress(3, None, 0, 0), "Reading the FBI list · page 3");
        assert_eq!(update_progress(26, Some(26), 120, 1200), "Checking poster photos · 120 of 1,200");
        assert_eq!(kept(1), "Kept 1 poster.");
        assert_eq!(skipped(12, "No face was found on the poster photo."), "12 left out: No face was found on the poster photo.");
        assert_eq!(face_label(11, S::ChipCandidate), "Face from frame 12. Possible candidate.");
        assert_eq!(clock(42.4), "0:42");
        assert_eq!(clock(3723.0), "1:02:03");
        assert_eq!(file_size(3100), "3.1 KB");
        assert_eq!(file_size(12_300_000), "12.3 MB");
    }
}
