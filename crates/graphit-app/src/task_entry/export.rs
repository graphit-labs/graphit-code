//! Complete audit export over the same pinned native reads as the Browser.
use super::*;
use serde_json::json;
use std::{
    collections::HashSet,
    io::{Seek, Write},
    num::NonZeroUsize,
};

#[derive(clap::Args)]
pub struct Arguments {
    #[command(flatten)]
    pub selection: CallArguments,
    #[arg(long, value_parser=super::identifier)]
    pub id: String,
    /// Publish this JSON file only after every audit page has reached EOF.
    #[arg(long, value_name = "FILE")]
    pub output: PathBuf,
    /// Records requested per audit page; every page is exported through EOF.
    #[arg(long, default_value = "20")]
    pub page_size: NonZeroUsize,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
async fn read(
    connection: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    mut call: CallToolRequestParams,
    original: &RequestContext<'_>,
) -> io::Result<Value> {
    crate::mcp_output::json_call(&mut call);
    let result = tokio::select! {
        result=connection.peer().call_tool(call)=>result.map_err(io::Error::other)?,
        ()=original.cancellation().cancelled()=>return Err(io::ErrorKind::Interrupted.into()),
        ()=tokio::time::sleep_until(original.snapshot().required_deadline()?.into())=>return Err(io::ErrorKind::TimedOut.into()),
    };
    if result.is_error == Some(true) {
        return Err(io::Error::other(format!(
            "Audit export incomplete: {}",
            serde_json::to_string(&result)?
        )));
    }
    original.check()?;
    let value = result
        .structured_content
        .ok_or_else(|| invalid("Audit export incomplete: missing native response"))?;
    value
        .get("result")
        .filter(|v| !v.is_null())
        .cloned()
        .ok_or_else(|| invalid("Audit export incomplete: record absent"))
}
fn page_call(
    base: &CallToolRequestParams,
    section: &str,
    cursor: &Value,
    size: NonZeroUsize,
) -> CallToolRequestParams {
    let mut call = base.clone();
    let args = call.arguments.as_mut().expect("prepared arguments");
    args.get_mut("input").expect("prepared input")[section] =
        json!({"cursor":cursor,"page_size":size});
    call
}
pub async fn run(
    installation: &InstallationPaths,
    client: &reqwest::Client,
    base: CallToolRequestParams,
    args: &Arguments,
    original: &RequestContext<'_>,
    output: &mut impl Write,
) -> io::Result<()> {
    original.check()?;
    let destination = if args.output.is_absolute() {
        args.output.clone()
    } else {
        std::env::current_dir()?.join(&args.output)
    };
    let parent = destination
        .parent()
        .ok_or_else(|| invalid("output needs a parent directory"))?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".audit-")
        .tempfile_in(parent)?;
    let address = crate::source_entry::address(installation, original).await?;
    let connection =
        crate::ast_source::connect_source(installation, address, client, original, ()).await?;
    let collect=async {
        let initial=read(&connection,base.clone(),original).await?;
        let session=base.name=="graphit_task_session_get";
        let kind=if session{"session"}else{"task"};
        let generation=initial.get("generation").ok_or_else(||invalid("missing audit generation"))?;
        let mut current=json!({"generation":generation,"creation":initial["creation"],"last_event":initial["last_event"]});
        current[kind]=initial.get(kind).filter(|v|!v.is_null()).ok_or_else(||invalid("missing audit record"))?.clone();
        let serialized=serde_json::to_vec(&current)?;
        temporary.write_all(&serialized[..serialized.len()-1])?;
        for section in if session {&["history"][..]}else{&["history","comments"][..]} {
            write!(temporary,",\"{section}\":{{")?;
            let field=if *section=="history"{"events"}else{"comments"};
            let start=format!("{section}_start_cursor");
            let mut cursor=initial.get(&start).filter(|v|v.is_string()).cloned().ok_or_else(||invalid("missing audit start cursor"))?;
            let mut seen=HashSet::new();
            let mut first_page=true;
            let mut has_records=false;
            loop {
                original.check()?;
                if !seen.insert(cursor.clone().to_string()){return Err(invalid("Audit export incomplete: repeated cursor"));}
                let next=read(&connection,page_call(&base,section,&cursor,args.page_size),original).await?;
                if next.get("generation")!=Some(generation) {return Err(invalid("Audit export incomplete: generation changed"));}
                let page=next.get(*section).filter(|v|v.is_object()).ok_or_else(||invalid("Audit export incomplete: requested section missing"))?;
                if first_page {
                    if *section=="history" {temporary.write_all(b"\"before\":")?;serde_json::to_writer(&mut temporary,page.get("before").ok_or_else(||invalid("history predecessor missing"))?)?;temporary.write_all(b",")?;}
                    write!(temporary,"\"{field}\":[")?;first_page=false;
                }
                for record in page.get(field).and_then(Value::as_array).ok_or_else(||invalid("Audit export incomplete: page records missing"))? {
                    if has_records {temporary.write_all(b",")?;}serde_json::to_writer(&mut temporary,record)?;has_records=true;
                }
                let next=page.get("next_cursor").ok_or_else(||invalid("Audit export incomplete: EOF marker missing"))?;
                if next.is_null(){break;}
                if !next.is_string(){return Err(invalid("invalid audit continuation"));}cursor=next.clone();
            }
            temporary.write_all(b"],\"next_cursor\":null}")?;
        }
        if session {
            // A separate pinned catalogue: no global transaction is promised.
            temporary.write_all(b",\"linked_tasks\":{")?;
            let mut cursor=Value::Null;let mut generation=None;let mut first=true;let mut has_rows=false;let mut seen=HashSet::new();
            loop {
                let mut input=json!({"filter":{"session_id":args.id},"page_size":args.page_size});
                if !cursor.is_null(){input["cursor"]=cursor.clone();}
                let mut call=CallToolRequestParams::new("graphit_task_list");
                let mut wire=base.arguments.clone().unwrap_or_default();wire.insert("input".into(),input);call.arguments=Some(wire);
                let page=read(&connection,call,original).await?;
                if !page.get("generation").is_some_and(Value::is_object) {
                    return Err(invalid("Audit export incomplete: linked Task catalogue has no published generation"));
                }
                if first {temporary.write_all(b"\"generation\":")?;serde_json::to_writer(&mut temporary,&page["generation"])?;temporary.write_all(b",\"tasks\":[")?;generation=Some(page["generation"].clone());first=false;}
                if generation.as_ref()!=page.get("generation"){return Err(invalid("linked Task catalogue generation changed"));}
                for row in page.get("results").and_then(Value::as_array).ok_or_else(||invalid("linked Task catalogue missing rows"))? {
                    if has_rows{temporary.write_all(b",")?;}serde_json::to_writer(&mut temporary,row)?;has_rows=true;
                }
                cursor=page.get("next_cursor").ok_or_else(||invalid("linked Task catalogue EOF missing"))?.clone();
                if cursor.is_null(){break;}
                if !seen.insert(cursor.to_string()){return Err(invalid("linked Task catalogue repeated cursor"));}
            }
            temporary.write_all(b"],\"next_cursor\":null}")?;
        }
        temporary.write_all(b"}\n")?;temporary.flush()?;original.check()?;Ok::<_,io::Error>(())
    }.await;
    let closed = connection.cancel().await.map_err(io::Error::other);
    collect?;
    closed?;
    original.check()?;
    temporary.as_file_mut().rewind()?;
    graphit_core::local_files::atomic_replace_with(
        &destination,
        None,
        original.cancellation(),
        |file| {
            io::copy(temporary.as_file_mut(), file)?;
            original.check()?;
            Ok(())
        },
    )?;
    serde_json::to_writer(
        &mut *output,
        &json!({"state":"completed","output":destination,"kind":if base.name=="graphit_task_session_get"{"session"}else{"task"},"id":args.id}),
    )?;
    writeln!(output)?;
    Ok(())
}
