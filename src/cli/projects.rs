use super::maps::cli_writer;
use crate::app::AppService;
use crate::core::{EventStore, Name, SchemaStore};
use crate::shared::Result;

pub(super) async fn init<S: EventStore, M: SchemaStore>(service: &AppService<S, M>, name: Option<&str>) -> Result<()> {
    let (actor, source) = cli_writer().await?;

    let name = match name {
        Some(typed) => Name::new(typed)?,
        None => Name::from_root(&source.path).ok_or("pass a name: percept init <name>")?,
    };

    let name = service.create_project(source, actor, name).await?;
    println!("Created project {}.", name.as_str());

    Ok(())
}
