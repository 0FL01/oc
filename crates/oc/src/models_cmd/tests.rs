use super::*;
use oc_adapters::composition::CatalogListing;

struct FailedWriter;
impl Write for FailedWriter {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }
}

#[tokio::test]
async fn catalog_output_failure_is_not_success_even_after_prefix() {
    let listing = CatalogListing {
        references: vec!["p/model".into()],
        diagnostics: vec![],
        complete: true,
    };
    let mut err = Vec::new();
    assert_eq!(
        emit(listing, &mut FailedWriter, &mut err).await,
        ExitCode::from(1)
    );
    assert_eq!(err, b"error: catalog output failed\n");
}

#[tokio::test]
async fn catalog_empty_and_flush_failure_are_distinct() {
    let empty = || CatalogListing {
        references: vec![],
        diagnostics: vec![],
        complete: true,
    };
    let mut out = Vec::new();
    assert_eq!(
        emit(empty(), &mut out, &mut Vec::new()).await,
        ExitCode::SUCCESS
    );
    assert!(out.is_empty());
    assert_eq!(
        emit(empty(), &mut FailedWriter, &mut Vec::new()).await,
        ExitCode::from(1)
    );
}
