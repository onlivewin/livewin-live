#ifndef PIC_COMMON_H
#define PIC_COMMON_H

#include <libavcodec/avcodec.h>
#include <libavutil/common.h>
#include <libavutil/samplefmt.h>
#include <libavformat/avformat.h>
int video_decode(uint8_t *data,int size,char *file_name)
{
    AVCodec *codec;
    AVCodecContext *c= NULL;
    int frame_count=0;
    AVFrame *frame;
    AVPacket avpkt;
    av_init_packet(&avpkt);
    codec = avcodec_find_decoder(AV_CODEC_ID_H264);
    c = avcodec_alloc_context3(codec);
    if (avcodec_open2(c, codec, NULL) < 0) {
        fprintf(stderr, "Could not open codec\n");
        return -1;
    }
    frame = av_frame_alloc();
    avpkt.data = data;
    avpkt.size = size;

    int ret, got_frame;
    ret = avcodec_send_packet(c, &avpkt); 
    if (ret < 0 || ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
        return -1;
    }

    avcodec_send_packet(c, NULL); 
    while (ret  >= 0) {
        ret = avcodec_receive_frame(c, frame);
        if (ret == 0) {
            got_frame =1;
            break;
        }

        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            break;
        }
    } 
    ret = -1;
    if (got_frame) {
       ret =  save_picture(frame,file_name);
    }

    av_frame_free(&frame);
    //avcodec_close(codec);
    avcodec_close(c);
    
    return ret;
}


int save_picture(AVFrame *pFrame, char *file_name) {//编码保存图片
    int width = pFrame->width;
    int height = pFrame->height;
    AVCodecContext *pCodeCtx = NULL;
    
    AVFormatContext *pFormatCtx = avformat_alloc_context();
    // 设置输出文件格式
    pFormatCtx->oformat = av_guess_format("mjpeg", NULL, NULL);
    // 创建并初始化输出AVIOContext
    if (avio_open(&pFormatCtx->pb, file_name, AVIO_FLAG_READ_WRITE) < 0) {
        printf("Couldn't open output file.");
        return -1;
    }
 
    // 构建一个新stream
    AVStream *pAVStream = avformat_new_stream(pFormatCtx, 0);
    if (pAVStream == NULL) {
        return -1;
    }
 
    AVCodecParameters *parameters = pAVStream->codecpar;
    parameters->codec_id = pFormatCtx->oformat->video_codec;
    parameters->codec_type = AVMEDIA_TYPE_VIDEO;
    parameters->format = AV_PIX_FMT_YUVJ420P;
    parameters->width = pFrame->width;
    parameters->height = pFrame->height;
 
    AVCodec *pCodec = avcodec_find_encoder(pAVStream->codecpar->codec_id);
 
    if (!pCodec) {
        printf("Could not find encoder\n");
        return -1;
    }
 
    pCodeCtx = avcodec_alloc_context3(pCodec);
    if (!pCodeCtx) {
        fprintf(stderr, "Could not allocate video codec context\n");
        return -1;
    }
 
    if ((avcodec_parameters_to_context(pCodeCtx, pAVStream->codecpar)) < 0) {
        fprintf(stderr, "Failed to copy %s codec parameters to decoder context\n",
                av_get_media_type_string(AVMEDIA_TYPE_VIDEO));
        return -1;
    }
 
    pCodeCtx->time_base = (AVRational) {1, 25};
 
    if (avcodec_open2(pCodeCtx, pCodec, NULL) < 0) {
        printf("Could not open codec.");
        return -1;
    }
 
    int ret = avformat_write_header(pFormatCtx, NULL);
    if (ret < 0) {
        printf("write_header fail\n");
        return -1;
    }
 
    int y_size = width * height;
 
    //Encode
    // 给AVPacket分配足够大的空间
    AVPacket pkt;
    av_new_packet(&pkt, y_size * 3);
 
    // 编码数据
    ret = avcodec_send_frame(pCodeCtx, pFrame);
    if (ret < 0) {
        printf("Could not avcodec_send_frame.");
        return -1;
    }
 
    // 得到编码后数据
    ret = avcodec_receive_packet(pCodeCtx, &pkt);
    if (ret < 0) {
        printf("Could not avcodec_receive_packet");
        return -1;
    }
 
    ret = av_write_frame(pFormatCtx, &pkt);
 
    if (ret < 0) {
        printf("Could not av_write_frame");
        return -1;
    }
 
    av_packet_unref(&pkt);
 
    //Write Trailer
    av_write_trailer(pFormatCtx);
 
    avcodec_close(pCodeCtx);
    avio_close(pFormatCtx->pb);
    avformat_free_context(pFormatCtx);
    return 0;
}
#endif //PIC_COMMON_H