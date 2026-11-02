//! An actual native encode probe, run in the host's admitted CPU pool.
pub async fn available(pool: &crate::runtime::cpu_pool::CpuPool) -> bool {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pool;
        false
    }
    #[cfg(target_os = "macos")]
    {
        pool.execute(tokio_util::sync::CancellationToken::new(), move |_| {
            let result = (|| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                let mut movie = video_kadr_vt::movie::HevcMovie::new_with_rate_control(
                    std::io::Cursor::new(Vec::new()),
                    false,
                    64,
                    48,
                    video_kadr_vt::rate::RateControl {
                        mode: video_kadr_vt::rate::Mode::Vbr {
                            max_bps: 30_000_000,
                        },
                        target_bps: 20_000_000,
                        rate_num: 24,
                        rate_den: 1,
                    },
                    48,
                )?;
                movie.frame(&[255; 64 * 48 * 4])?;
                movie.finish()?;
                Ok(())
            })();
            Ok(result.is_ok())
        })
        .await
        .unwrap_or(false)
    }
}
