#[warn(unused_mut)]
use anyhow::Result;
use chrono::Local;
use std::io::Write;
use tokio::sync::mpsc;
#[cfg(feature = "flv")]
use xlive::flv;
#[cfg(feature = "hls")]
use xlive::hls;
#[cfg(feature = "http-flv")]
use xlive::http_flv;
use xlive::service::Service;
use xlive::transport::TsMessageQueue;
#[cfg(feature = "hls")]
use xlive::ts;

use xlive::user::Redis;
use xlive::Manager;

#[tokio::main]
async fn main() -> Result<()> {
    let config = xlive::config::get_setting();

    let env =
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, config.log_level);
    env_logger::Builder::from_env(env)
        .format(|buf, record| {
            writeln!(
                buf,
                "{} {} [{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.module_path().unwrap_or("<unnamed>"),
                &record.args()
            )
        })
        .init();

    let mut handles = Vec::new();
    let redis_client: Option<Redis> = Some(Redis::new(&config.redis)?);

    // 初始化全局速率限制器
    xlive::rate_limiter::init_global_rate_limiter(&config.rate_limit);
    log::info!("Rate limiter initialized with config: connection={}/{}, hls_request={}/{}, stream_creation={}/{}",
        config.rate_limit.connection.max_requests, config.rate_limit.connection.window_duration_secs,
        config.rate_limit.hls_request.max_requests, config.rate_limit.hls_request.window_duration_secs,
        config.rate_limit.stream_creation.max_requests, config.rate_limit.stream_creation.window_duration_secs);

    let manager = Manager::new(redis_client, config.full_gop, config.auth_enable);
    let manager_handle = manager.handle();
    handles.push(tokio::spawn(manager.run()));

    #[cfg(feature = "flv")]
    {
        let manager_handle_t = manager_handle.clone();
        let data_path = config.flv.data_path;
        handles.push(tokio::spawn(async {
           _ = flv::Service::new(manager_handle_t, data_path).run().await;
        }));
    }
    #[cfg(feature = "http-flv")]
    {
        let port = config.http_flv.port;
        let manager_handle_t = manager_handle.clone();
        handles.push(tokio::spawn(async move {
            http_flv::Service::new(manager_handle_t).run(port).await;
        }));
    }

    #[cfg(feature = "hls")]
    {
        let (mq_handle, mq_receiver) = mpsc::unbounded_channel::<TsMessageQueue>();
        let manager_handle_t = manager_handle.clone();
        let data_path = config.hls.data_path;
        let ts_duration = config.hls.ts_duration;
        let port = config.hls.port;
        handles.push(tokio::spawn(async move {
            _ = ts::Service::new(manager_handle_t, data_path, mq_handle, ts_duration)
                .run()
                .await;
        }));

        handles.push(tokio::spawn(async move {
            _ = hls::run(mq_receiver, port as u32).await;
        }));
    }
    let port = config.rtmp.port;
    handles.push(tokio::spawn(Service::new(manager_handle).run(port)));

    for handle in handles {
        handle.await?;
    }
    Ok(())
}
