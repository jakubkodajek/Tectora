//! How a core run ends when it does not produce its normal output.
//!
//! The original has two error paths. A *fatal* error (file and format
//! errors, an interval error, a broken wall chain) shows a message box and
//! jumps back to the pipeline, which returns the code (1 and 2) or, for a
//! broken wall chain (3), writes a fallback SOL. A *cut* error of the
//! sweep (code 2 for the messages below) shows "E%05d : <message>" and
//! ends the model with that code; DACAD then reports the failure as
//! E00002.

/// A message of the core, with its id in DACAD's string table when it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Msg {
    pub id: Option<u32>,
    pub text: &'static str,
}

impl std::fmt::Display for Msg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.text)
    }
}

macro_rules! msgs {
    ($($name:ident = $id:expr, $text:expr;)*) => {
        $(pub const $name: Msg = Msg { id: $id, text: $text };)*
    };
}

msgs! {
    MSG_OPEN = Some(0x9f6), "Fehler beim Öffnen der Datei";
    MSG_FORMAT = Some(0xf120), "Unerwartetes Dateiformat.";
    MSG_WALL_DEF = Some(0x9fe), "RD2-Datei - Fehler in Wand-Definition";
    MSG_ROOF_DEF = Some(0x9ff), "RD2-Datei - Fehler in Dach-Definition";
    MSG_HOLE_DEF = Some(0xa00), "RD2-Datei - Fehler in Öffnungs-Definition";
    MSG_L_ERROR = None, "L error";
    MSG_NO_PLAN = Some(0xa01), "Kein geschlossener Grundriss";
    MSG_SINGLE_FACE = Some(0xa02), "Einzelne Fläche vorhanden";
    MSG_EAVE_CUT = Some(0xa09), "Schnittfehler";
    MSG_OPEN_FACES = Some(0xa0f), "Nicht geschlossene Fläche(n) vorhanden";
    MSG_NO_SOLUTION = Some(0xa10), "Keine Lösung gefunden";
    MSG_WRITE = Some(0xf126), "Beim Schreiben ist ein unerwarteter Fehler aufgetreten";
    MSG_INTERVAL = None, "Intervallfehler";
}

/// Why a core run stopped.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoreError {
    /// A fatal error with the pipeline's return code (1 file/format, 2 interval).
    #[error("core error {code}: {msg}")]
    Fatal { code: u32, msg: Msg },
    /// The wall chain does not close (the pipeline writes the fallback SOL, code 3).
    #[error("{msg}")]
    NoPlan { msg: Msg },
    /// A cut error of the sweep ("E%05d : message").
    #[error("E{code:05} : {msg}")]
    Cut { code: u32, msg: Msg },
    /// No model could be processed (progress 0xa10; cancelled there).
    #[error("{0}")]
    NoSolution(Msg),
    /// A stage not ported yet.
    #[error("dcore: {0} is not ported yet")]
    Unimplemented(&'static str),
    /// The original would fault here (null pointer, missing record).
    #[error("dcore: the original faults: {0}")]
    Fault(&'static str),
}

impl CoreError {
    pub fn fatal(code: u32, msg: Msg) -> CoreError {
        CoreError::Fatal { code, msg }
    }

    /// The error code DACAD shows for this outcome (`E00002` for every cut
    /// error and for an unsolved model), if any.
    pub fn dacad_code(&self) -> Option<String> {
        match self {
            CoreError::Cut { code, .. } => Some(format!("E{code:05}")),
            CoreError::NoSolution(_) => Some("E00002".into()),
            _ => None,
        }
    }
}
