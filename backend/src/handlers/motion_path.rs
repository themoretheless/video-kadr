//! Bounded native spatial authoring and preview calculations.
use crate::domain::motion_path::{
    MotionPath, PathLocation, PathNodePart, Point, SpatialTangentMode,
};
use crate::error::{ApiJson, AppError, AppResult};
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum MotionPathRequest {
    Nearest {
        path: MotionPath,
        point: Point,
    },
    Translate {
        path: MotionPath,
        delta: Point,
    },
    Remove {
        path: MotionPath,
        index: usize,
    },
    ToggleSegment {
        path: MotionPath,
        index: usize,
    },
    Solve {
        path: MotionPath,
    },
    Measure {
        path: MotionPath,
    },
    Sample {
        path: MotionPath,
        progresses: Vec<f64>,
    },
    Edit {
        path: MotionPath,
        index: usize,
        part: PathNodePart,
        value: Point,
    },
    Mode {
        path: MotionPath,
        index: usize,
        mode: SpatialTangentMode,
    },
    Insert {
        path: MotionPath,
        segment: usize,
        t: f64,
        id: String,
    },
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionPathResponse {
    path: MotionPath,
    length: f64,
    points: Vec<Point>,
    node_progresses: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nearest: Option<PathLocation>,
}
fn evaluate(request: MotionPathRequest) -> Result<MotionPathResponse, String> {
    let hit_point = match &request {
        MotionPathRequest::Nearest { point, .. } => Some(*point),
        _ => None,
    };
    let (path, progresses) = match request {
        MotionPathRequest::Nearest { path, .. } => (path, vec![]),
        MotionPathRequest::Translate { path, delta } => (path.translate(delta)?, vec![]),
        MotionPathRequest::Remove { path, index } => (path.remove_node(index)?, vec![]),
        MotionPathRequest::ToggleSegment { path, index } => (path.toggle_segment(index)?, vec![]),
        MotionPathRequest::Measure { path } => (path, vec![]),
        MotionPathRequest::Solve { path } => (path.solve()?, vec![]),
        MotionPathRequest::Sample { path, progresses } => {
            if progresses.len() > 4096 || progresses.iter().any(|progress| !progress.is_finite()) {
                return Err("Preview requires at most4096 finite progress samples".into());
            }
            (path, progresses)
        }
        MotionPathRequest::Edit {
            path,
            index,
            part,
            value,
        } => (path.edit_node(index, part, value)?, vec![]),
        MotionPathRequest::Mode { path, index, mode } => (path.set_node_mode(index, mode)?, vec![]),
        MotionPathRequest::Insert {
            path,
            segment,
            t,
            id,
        } => (path.insert_node(segment, t, id)?, vec![]),
    };
    let compiled = path.compile()?;
    let points: Vec<_> = progresses
        .into_iter()
        .map(|progress| compiled.sample(progress))
        .collect();
    if points
        .iter()
        .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return Err("Preview position exceeds finite range".into());
    }
    let node_progresses = (0..path.nodes.len())
        .map(|index| {
            if compiled.length() == 0.0 {
                return 0.0;
            }
            compiled
                .samples()
                .iter()
                .find(|sample| sample[3] == index as f64)
                .map(|sample| sample[2] / compiled.length())
                .unwrap_or(0.0)
        })
        .collect();
    let nearest = hit_point
        .map(|point| path.nearest_location(point))
        .transpose()?
        .flatten();
    Ok(MotionPathResponse {
        path,
        length: compiled.length(),
        points,
        nearest,
        node_progresses,
    })
}
pub async fn motion_path_handler(
    ApiJson(request): ApiJson<MotionPathRequest>,
) -> AppResult<Json<MotionPathResponse>> {
    let response = tokio::task::spawn_blocking(move || evaluate(request))
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?
        .map_err(AppError::bad_request)?;
    Ok(Json(response))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn path() -> MotionPath {
        serde_json::from_value(serde_json::json!({"nodes":[{"id":"a","point":{"x":0,"y":0},"inHandle":{"x":0,"y":0},"outHandle":{"x":0,"y":0},"segment":"line","tangentMode":"corner"},{"id":"b","point":{"x":100,"y":0},"inHandle":{"x":0,"y":0},"outHandle":{"x":0,"y":0},"segment":"line","tangentMode":"corner"}]})).unwrap()
    }
    #[tokio::test]
    async fn handler_executes_native_sampling_and_authoring_and_refuses_bad_requests() {
        let response = motion_path_handler(ApiJson(MotionPathRequest::Sample {
            path: path(),
            progresses: vec![-0.1, 0.5, 1.1],
        }))
        .await
        .unwrap()
        .0;
        let curved = evaluate(MotionPathRequest::ToggleSegment {
            path: path(),
            index: 0,
        })
        .unwrap();
        assert!(curved.length > 100.0);
        let moved = evaluate(MotionPathRequest::Translate {
            path: curved.path.clone(),
            delta: Point { x: 13.0, y: -9.0 },
        })
        .unwrap();
        assert!((moved.length - curved.length).abs() < 1e-10);
        assert_eq!(
            moved.path.nodes[0].out_handle,
            curved.path.nodes[0].out_handle
        );
        let original = curved.path.compile().unwrap();
        let translated = moved.path.compile().unwrap();
        for step in -10..=110 {
            let old = original.sample(step as f64 / 100.0);
            let new = translated.sample(step as f64 / 100.0);
            assert!((new.x - old.x - 13.0).abs() < 1e-10);
            assert!((new.y - old.y + 9.0).abs() < 1e-10);
        }
        assert!(curved
            .path
            .translate(Point {
                x: f64::MAX,
                y: 0.0
            })
            .is_err());

        assert_eq!(curved.path.nodes[0].out_handle, Point { x: 60.0, y: -60.0 });
        let removed = evaluate(MotionPathRequest::Remove {
            path: curved.path.clone(),
            index: 1,
        })
        .unwrap();
        assert_eq!(removed.path.nodes.len(), 1);
        assert_eq!(removed.length, 0.0);
        assert!(evaluate(MotionPathRequest::Remove {
            path: removed.path,
            index: 0
        })
        .is_err());
        assert!(evaluate(MotionPathRequest::ToggleSegment {
            path: curved.path,
            index: usize::MAX
        })
        .is_err());
        let hit = evaluate(MotionPathRequest::Nearest {
            path: path(),
            point: Point { x: 37.0, y: 8.0 },
        })
        .unwrap()
        .nearest
        .unwrap();
        assert_eq!(hit.segment, 0);
        assert!((hit.t - 0.37).abs() < 1e-6);
        assert!((hit.distance - 8.0).abs() < 1e-8);
        assert_eq!(response.length, 100.0);
        assert_eq!(response.node_progresses, vec![0.0, 1.0]);
        assert_eq!(response.points[1], Point { x: 50.0, y: 0.0 });
        assert!((response.points[2].x - 110.0).abs() < 1e-12);
        let inserted = motion_path_handler(ApiJson(MotionPathRequest::Insert {
            path: path(),
            segment: 0,
            t: 0.25,
            id: "new".into(),
        }))
        .await
        .unwrap()
        .0;
        assert_eq!(inserted.path.nodes[1].point, Point { x: 25.0, y: 0.0 });
        assert_eq!(inserted.node_progresses, vec![0.0, 0.25, 1.0]);
        assert!(motion_path_handler(ApiJson(MotionPathRequest::Sample {
            path: path(),
            progresses: vec![0.0; 4097]
        }))
        .await
        .is_err());
        assert!(motion_path_handler(ApiJson(MotionPathRequest::Edit {
            path: path(),
            index: 9,
            part: PathNodePart::Point,
            value: Point::ZERO
        }))
        .await
        .is_err());
        assert!(serde_json::from_value::<MotionPathRequest>(
            serde_json::json!({"operation":"solve","path":{"nodes":[]},"unknown":true})
        )
        .is_err());
    }
}
