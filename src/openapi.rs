use utoipa::OpenApi;

use crate::errors;
use crate::handlers;
use crate::models;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "RUC Finder API",
        description = "REST API for querying RUC (Registro Unico de Contribuyentes) data from DNIT Paraguay.\n\nData is scraped from the official DNIT website, parsed from ZIP/TXT files, and stored in PostgreSQL. The API supports exact lookup, filtered search with pagination, and fuzzy (trigram similarity) search.\n\n**Data source:** [DNIT Paraguay — Listado de RUC con sus equivalencias](https://www.dnit.gov.py/web/portal-institucional/listado-de-ruc-con-sus-equivalencias)",
        version = "0.1.1",
        contact(name = "GitHub Repository", url = "https://github.com/EchoSistema/py-ruc-finder"),
        license(name = "MIT", identifier = "MIT")
    ),
    tags(
        (name = "RUC", description = "Endpoints for querying and searching RUC (tax ID) records. Supports exact lookup by number, filtered search with pagination, and fuzzy name matching via pg_trgm."),
        (name = "System", description = "Operational endpoints for health checks and data synchronization.")
    ),
    paths(
        handlers::health_check,
        handlers::get_ruc_by_number,
        handlers::search_ruc,
        handlers::fuzzy_search_ruc,
        handlers::trigger_sync,
    ),
    components(schemas(
        models::Ruc,
        models::RucWithScore,
        errors::ErrorResponse,
    ))
)]
pub struct ApiDoc;
