use super::response::JSON_CONTENT_TYPE;
use gateway_plugin_sdk::call::management::{
    ManagementPage, ManagementRegistration, ManagementResource, ManagementRoute,
};

pub(crate) fn registration() -> ManagementRegistration {
    let get = |path: &str| ManagementRoute {
        method: "GET".to_owned(),
        path: path.to_owned(),
        request_content_types: Vec::new(),
        response_content_types: vec![JSON_CONTENT_TYPE.to_owned()],
    };
    let post = |path: &str| ManagementRoute {
        method: "POST".to_owned(),
        path: path.to_owned(),
        request_content_types: vec![JSON_CONTENT_TYPE.to_owned()],
        response_content_types: vec![JSON_CONTENT_TYPE.to_owned()],
    };
    ManagementRegistration {
        routes: vec![
            get("api/bootstrap"),
            post("api/models"),
            post("api/results"),
            post("api/questions"),
            get("api/jobs/current"),
            post("api/jobs"),
            post("api/jobs/stop"),
        ],
        resources: ["web/index.html", "web/app.js", "web/app.css"]
            .into_iter()
            .map(|path| ManagementResource {
                path: path.to_owned(),
                public: path == "web/app.css",
            })
            .collect(),
        pages: vec![ManagementPage {
            id: "intelligence-check".to_owned(),
            title: "模型智力检测".to_owned(),
            description: Some("按账号检测模型推理是否正常".to_owned()),
            entry: "web/index.html".to_owned(),
            icon: None,
        }],
        callbacks: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::registration;

    #[test]
    fn registration_lists_routes() {
        let registration = registration();
        let routes: Vec<_> = registration
            .routes
            .iter()
            .map(|route| (route.method.as_str(), route.path.as_str()))
            .collect();
        assert_eq!(
            routes,
            [
                ("GET", "api/bootstrap"),
                ("POST", "api/models"),
                ("POST", "api/results"),
                ("POST", "api/questions"),
                ("GET", "api/jobs/current"),
                ("POST", "api/jobs"),
                ("POST", "api/jobs/stop"),
            ]
        );
        assert_eq!(registration.pages[0].entry, "web/index.html");
    }
}
