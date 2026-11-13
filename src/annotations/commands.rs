use super::{Annotation, AnnotationBook, ExportLink, TextAnchor};

#[derive(Clone, Debug)]
pub struct AnnotationDraft {
    pub anchor: TextAnchor,
    pub note: String,
}

#[derive(Clone, Debug)]
pub struct NoteUpdate {
    pub annotation_id: String,
    pub note: String,
}

#[derive(Clone, Debug)]
pub enum AnnotationCommand {
    Create(AnnotationDraft),
    EditNote(NoteUpdate),
    Delete(String),
}

#[derive(Clone, Debug)]
pub struct AnnotationJob {
    pub request_id: u64,
    pub command: AnnotationCommand,
}

#[derive(Clone, Debug, Default)]
pub struct ExportState {
    pub running: bool,
    pub link: Option<ExportLink>,
    pub error: Option<String>,
}

impl ExportState {
    pub fn pending(&self) -> bool {
        self.running
            || self.error.is_some()
            || self
                .link
                .as_ref()
                .is_some_and(|link| link.revision != link.exported_revision)
    }
}

#[derive(Clone, Debug)]
pub struct AnnotationResult {
    pub request_id: u64,
    pub annotations: Vec<Annotation>,
    pub saved_id: Option<String>,
    pub error: Option<String>,
    pub export: ExportState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Destination {
    pub path: String,
    pub exists: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverwriteApproval {
    pub link: ExportLink,
    pub file_digest: String,
}

#[derive(Clone, Debug)]
pub enum LinkCommand {
    Status,
    Inspect(String),
    Set {
        destination: Destination,
        create: bool,
    },
    Refresh,
    PrepareOverwrite,
    Overwrite(OverwriteApproval),
    Unlink,
}

#[derive(Clone, Debug)]
pub struct LinkJob {
    pub request_id: u64,
    pub command: LinkCommand,
}

#[derive(Clone, Debug)]
pub enum LinkReply {
    Destination(Destination),
    Overwrite(OverwriteApproval),
    State(ExportState),
}

#[derive(Clone, Debug)]
pub struct LinkResult {
    pub request_id: u64,
    pub reply: std::result::Result<LinkReply, String>,
}

#[derive(Clone, Debug)]
pub struct ReassociationSource {
    pub book: AnnotationBook,
    pub annotation_count: usize,
}
#[derive(Clone, Debug)]
pub enum ReassociationCommand {
    Sources,
    Copy(String),
}
#[derive(Clone, Debug)]
pub struct ReassociationJob {
    pub request_id: u64,
    pub command: ReassociationCommand,
}
#[derive(Clone, Debug)]
pub struct ReassociationReport {
    pub annotations: Vec<Annotation>,
    pub copied_count: usize,
    pub skipped_count: usize,
    pub first_copied_id: Option<String>,
    pub export: ExportState,
}
#[derive(Clone, Debug)]
pub enum ReassociationReply {
    Sources(Vec<ReassociationSource>),
    Copied(ReassociationReport),
}
#[derive(Clone, Debug)]
pub struct ReassociationResult {
    pub request_id: u64,
    pub reply: std::result::Result<ReassociationReply, String>,
}
