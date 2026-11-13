use crate::{Error, Result};

pub(super) fn join_resource(base_document: &str, reference: &str) -> Result<String> {
    if reference.starts_with('/')
        || reference
            .split('/')
            .next()
            .is_some_and(|first| first.contains(':'))
    {
        return Err(Error::Compatibility(
            "An ebook resource cannot name an external or absolute path.".into(),
        ));
    }
    let base = base_document
        .rsplit_once('/')
        .map_or("", |(parent, _)| parent);
    let mut segments = base
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    for segment in reference.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err(Error::Compatibility(
                        "An ebook resource escapes its archive root.".into(),
                    ));
                }
            }
            _ => segments.push(segment),
        }
    }
    let path = segments.join("/");
    validate_resource_name(&path)?;
    Ok(path)
}

pub(super) fn validate_resource_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.starts_with('/')
        || name.contains(['\0', '\\'])
        || name.split('/').any(|segment| segment == "..")
    {
        return Err(Error::Compatibility(
            "The ebook resource path is unsafe.".into(),
        ));
    }
    Ok(())
}
