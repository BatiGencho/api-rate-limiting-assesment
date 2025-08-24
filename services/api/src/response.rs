use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};

pub struct ResponseWithHeaders<T> {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: T,
}

impl<T> ResponseWithHeaders<T> {
    pub fn new(status: StatusCode, headers: HeaderMap, body: T) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }
}

impl<T> IntoResponse for ResponseWithHeaders<T>
where
    T: IntoResponse,
{
    fn into_response(self) -> Response {
        let mut response = (self.status, self.body).into_response();
        response.headers_mut().extend(self.headers);
        response
    }
}
