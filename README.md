# LiveWin-Live

LiveWin(来玩) 直播服务器

## features

- [x] 实现rtmp推流,拉流,
- [x] 支持H264/H265 
- [x] 可配置支持gop cache 
- [x] 支持视频流录制存储成本地flv文件.
- [x] 支持http-flv.
- [x] 支持hls.
- [x] 集成redis publisher用户认证.
- [x] 支持关键帧转储成jpg(使用ffmpeg)


## 编译

### 默认编译特性

- rtmp 推流
- rtmp 拉流
- http-flv拉流
- hls 拉流

### 编译带用户认证

```bash
   cargo build --features "auth" --release
```

### 编译带flv本地录播

```bash
   cargo build --features "flv" --release
```

### 编译带切割成ts

```bash
   cargo build --features "ts" --release
```

### 编译关键帧转jpg(需要ffmpeg支持)

```bash
   cargo build --features "keyframe_image" --release
```

## usage

- 启动`xlive`
```
./xlive
```
- 推rtmp流（循环）
```
ffmpeg -re -stream_loop -1 -i ~/Videos/dde-introduction.mp4 -c copy -f flv rtmp://localhost:1935/{appname}/{key}
```
- rtmp拉流

可以使用vlc观看流视频
```
rtmp://localhost:1935/{appname}
```
- http-flv拉流

可以用vlc和web_player(基于flv.js)观看
```
http://localhost:3006/{appname}.flv
```

- hls拉流

可以用vlc和web_player(基于flv.js)观看
```
http://localhost:3000/{appname}.m3u8
```