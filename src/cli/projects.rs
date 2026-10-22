use super::maps::cli_writer;
use crate::app::AppService;
use crate::core::{DefinitionStore, EventStore, Name};
use crate::shared::Result;

pub(super) async fn init<S: EventStore, M: DefinitionStore>(service: &AppService<S, M>, name: Option<&str>) -> Result<()> {
    let (actor, source) = cli_writer().await?;
    let root = source.path()?.to_owned();

    let name = match name {
        Some(typed) => Name::new(typed)?,
        None => Name::from_root(&root).ok_or("pass a name: percept init <name>")?,
    };

    let project = service.create_project(source, actor, name, Some(root)).await?;
    println!("Created project {}.", project.name.as_str());

    Ok(())
}
