/* libSceVideodec: H.264 access units decoded by FFmpeg into the game's NV12 frame buffers.
 * Structures, sizes and the resource estimate follow shadPS4's videodec (GPL-2.0-or-later);
 * pictures larger than the game's frame buffer are an error code instead of an overflow. */
#include "runtime.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <libavcodec/avcodec.h>
#include <libswscale/swscale.h>

#define ERR_API_FAIL ((int32_t)0x80C10000)
#define ERR_STRUCT_SIZE ((int32_t)0x80C10002)
#define ERR_HANDLE ((int32_t)0x80C10003)
#define ERR_AU_SIZE ((int32_t)0x80C10009)
#define ERR_AU_POINTER ((int32_t)0x80C1000A)
#define ERR_FRAME_BUFFER_SIZE ((int32_t)0x80C1000B)
#define ERR_FRAME_BUFFER_POINTER ((int32_t)0x80C1000C)
#define ERR_ARGUMENT_POINTER ((int32_t)0x80C1000F)

typedef struct {
    uint64_t this_size;
    uint32_t codec_type, profile, max_level;
    int32_t max_frame_width, max_frame_height, max_dpb_frame_count;
    uint64_t videodec_flags;
} ConfigInfo;
typedef struct {
    uint64_t this_size, cpu_memory_size;
    void *cpu_memory;
    uint64_t cpu_gpu_memory_size;
    void *cpu_gpu_memory;
    uint64_t max_frame_buffer_size;
    uint32_t frame_buffer_alignment;
} ResourceInfo;
typedef struct { uint64_t this_size; void *handle; uint64_t version; } Ctrl;
typedef struct { uint64_t this_size; void *frame_buffer; uint64_t frame_buffer_size; } FrameBuffer;
typedef struct {
    uint32_t num_units_in_tick, time_scale;
    uint8_t fixed_frame_rate_flag, aspect_ratio_idc;
    uint16_t sar_width, sar_height;
    uint8_t colour_primaries, transfer_characteristics, matrix_coefficients, video_full_range_flag;
    uint32_t crop_left, crop_right, crop_top, crop_bottom;
} AvcInfo;
typedef struct {
    uint64_t this_size;
    uint32_t is_valid, codec_type, frame_width, frame_pitch, frame_height, is_error_pic;
    uint64_t pts_data, attached_data;
    union { uint8_t reserved[64]; AvcInfo avc; } codec;
} PictureInfo;
typedef struct { uint64_t this_size; void *au_data; uint64_t au_size, pts_data, dts_data, attached_data; } InputData;
_Static_assert(sizeof(ConfigInfo)==40 && sizeof(ResourceInfo)==56 && sizeof(Ctrl)==24 && sizeof(FrameBuffer)==24 &&
               sizeof(PictureInfo)==112 && sizeof(InputData)==48,"Videodec guest layouts");

typedef struct { AVCodecContext *codec; AVPacket *packet; AVFrame *frame, *nv12; struct SwsContext *sws; } Decoder;
static size_t pictures_decoded;

static uint32_t align(uint32_t value, uint32_t to) { return (value+to-1)&~(to-1); }

static ABI int32_t videodec_query(const ConfigInfo *config, ResourceInfo *out) {
    if (!config || !out) return ERR_ARGUMENT_POINTER;
    if (config->this_size!=sizeof(*config) || out->this_size!=sizeof(*out)) return ERR_STRUCT_SIZE;
    int32_t width=config->max_frame_width, height=config->max_frame_height;
    if (width<=0 || height<=0) { width=config->max_level>=150 ? 3840 : 1920; height=config->max_level>=150 ? 2160 : 1080; }
    uint64_t frame=(uint64_t)align((uint32_t)width,256)*align((uint32_t)height,16)*3/2;
    uint64_t padded=((frame+255)&~UINT64_C(255))+0x4000;
    uint64_t surfaces=(uint64_t)(config->max_dpb_frame_count>0 ? config->max_dpb_frame_count : 8)+2;
    out->cpu_memory=NULL; out->cpu_gpu_memory=NULL;
    out->cpu_memory_size=16u<<20;
    out->cpu_gpu_memory_size=padded*surfaces+(8u<<20);
    out->max_frame_buffer_size=padded;
    out->frame_buffer_alignment=0x100;
    return 0;
}
static ABI int32_t videodec_create(const ConfigInfo *config, const ResourceInfo *resource, Ctrl *out) {
    if (!config || !resource || !out) return ERR_ARGUMENT_POINTER;
    if (config->this_size!=sizeof(*config) || resource->this_size!=sizeof(*resource)) return ERR_STRUCT_SIZE;
    const AVCodec *codec=avcodec_find_decoder(AV_CODEC_ID_H264);
    Decoder *d=calloc(1,sizeof(*d));
    if (!codec || !d) { free(d); return ERR_API_FAIL; }
    d->codec=avcodec_alloc_context3(codec);
    d->packet=av_packet_alloc();
    d->frame=av_frame_alloc();
    d->nv12=av_frame_alloc();
    if (d->codec) d->codec->flags|=AV_CODEC_FLAG_COPY_OPAQUE; /* attached data follows its picture */
    if (!d->codec || !d->packet || !d->frame || !d->nv12 || avcodec_open2(d->codec,codec,NULL)<0) {
        avcodec_free_context(&d->codec); av_packet_free(&d->packet); av_frame_free(&d->frame); av_frame_free(&d->nv12);
        free(d); return ERR_API_FAIL;
    }
    out->this_size=sizeof(*out); out->handle=d; out->version=1;
    return 0;
}

/* The decoded picture into the game's buffer: luma, then interleaved chroma, both with a pitch
 * of the width aligned to 64 and the height aligned to 16 (the last rows repeated). */
static int32_t picture(Decoder *d, FrameBuffer *buffer, PictureInfo *info) {
    AVFrame *f=d->frame;
    if (f->format!=AV_PIX_FMT_NV12) {
        av_frame_unref(d->nv12);
        d->nv12->format=AV_PIX_FMT_NV12; d->nv12->width=f->width; d->nv12->height=f->height;
        d->sws=sws_getCachedContext(d->sws,f->width,f->height,(enum AVPixelFormat)f->format,
                                    f->width,f->height,AV_PIX_FMT_NV12,SWS_FAST_BILINEAR,NULL,NULL,NULL);
        if (!d->sws || av_frame_get_buffer(d->nv12,0)<0 ||
            sws_scale(d->sws,(const uint8_t *const *)f->data,f->linesize,0,f->height,d->nv12->data,d->nv12->linesize)<0)
            return ERR_API_FAIL;
        f=d->nv12;
    }
    uint32_t width=(uint32_t)f->width, height=(uint32_t)f->height;
    uint32_t pitch=align(width,64), rows=align(height,16);
    if ((uint64_t)pitch*rows*3/2>buffer->frame_buffer_size) return ERR_FRAME_BUFFER_SIZE;
    unsigned char *luma=buffer->frame_buffer, *chroma=luma+(size_t)pitch*rows;
    for (uint32_t y=0;y<rows;++y) {
        uint32_t from=y<height ? y : height-1;
        memcpy(luma+(size_t)y*pitch,f->data[0]+(size_t)from*(size_t)f->linesize[0],width);
        if (y<rows/2) {
            uint32_t c=y<height/2 ? y : height/2-1;
            memcpy(chroma+(size_t)y*pitch,f->data[1]+(size_t)c*(size_t)f->linesize[1],width);
        }
    }
    info->is_valid=1; info->codec_type=0;
    info->frame_width=align(width,16); info->frame_pitch=pitch; info->frame_height=rows;
    info->is_error_pic=0;
    info->pts_data=(uint64_t)d->frame->pts;
    info->attached_data=(uint64_t)(uintptr_t)d->frame->opaque;
    memset(&info->codec,0,sizeof(info->codec));
    info->codec.avc.colour_primaries=(uint8_t)d->frame->color_primaries;
    info->codec.avc.crop_right=pitch-width;
    info->codec.avc.crop_bottom=rows-height;
    ++pictures_decoded;
    return 0;
}
static int32_t receive(Decoder *d, FrameBuffer *buffer, PictureInfo *info) {
    av_frame_unref(d->frame);
    int r=avcodec_receive_frame(d->codec,d->frame);
    if (r==AVERROR(EAGAIN) || r==AVERROR_EOF) return 0;
    return r<0 ? ERR_API_FAIL : picture(d,buffer,info);
}
static int32_t check(Ctrl *ctrl, FrameBuffer *buffer, PictureInfo *info) {
    if (!ctrl || !buffer || !info) return ERR_ARGUMENT_POINTER;
    if (ctrl->this_size!=sizeof(*ctrl) || buffer->this_size!=sizeof(*buffer) || info->this_size!=sizeof(*info)) return ERR_STRUCT_SIZE;
    if (!ctrl->handle) return ERR_HANDLE;
    if (!buffer->frame_buffer) return ERR_FRAME_BUFFER_POINTER;
    info->is_valid=0;
    return 0;
}
static ABI int32_t videodec_decode(Ctrl *ctrl, const InputData *input, FrameBuffer *buffer, PictureInfo *info) {
    if (!input) return ERR_ARGUMENT_POINTER;
    int32_t r=check(ctrl,buffer,info);
    if (r) return r;
    if (!input->au_data) return ERR_AU_POINTER;
    if (!input->au_size || input->au_size>INT32_MAX) return ERR_AU_SIZE;
    Decoder *d=ctrl->handle;
    d->packet->data=input->au_data; d->packet->size=(int)input->au_size;
    d->packet->pts=(int64_t)input->pts_data; d->packet->dts=(int64_t)input->dts_data;
    d->packet->opaque=(void *)(uintptr_t)input->attached_data;
    int sent=avcodec_send_packet(d->codec,d->packet);
    if (sent==AVERROR_EOF) { avcodec_flush_buffers(d->codec); sent=avcodec_send_packet(d->codec,d->packet); } /* after a flush */
    d->packet->data=NULL; d->packet->size=0;
    return sent<0 ? ERR_API_FAIL : receive(d,buffer,info);
}
/* One buffered picture per call; the game calls it until no picture comes back. */
static ABI int32_t videodec_flush(Ctrl *ctrl, FrameBuffer *buffer, PictureInfo *info) {
    int32_t r=check(ctrl,buffer,info);
    if (r) return r;
    Decoder *d=ctrl->handle;
    avcodec_send_packet(d->codec,NULL); /* AVERROR_EOF once draining: the rest still comes out */
    return receive(d,buffer,info);
}
static ABI int32_t videodec_reset(Ctrl *ctrl) {
    if (!ctrl) return ERR_ARGUMENT_POINTER;
    if (!ctrl->handle) return ERR_HANDLE;
    avcodec_flush_buffers(((Decoder *)ctrl->handle)->codec);
    return 0;
}
static ABI int32_t videodec_delete(Ctrl *ctrl) {
    if (!ctrl) return ERR_ARGUMENT_POINTER;
    Decoder *d=ctrl->handle;
    if (!d) return ERR_HANDLE;
    avcodec_free_context(&d->codec); av_packet_free(&d->packet); av_frame_free(&d->frame); av_frame_free(&d->nv12);
    sws_freeContext(d->sws);
    free(d);
    ctrl->handle=NULL;
    return 0;
}

static const RuntimeExport exports[]={
    {"sceVideodecQueryResourceInfo",videodec_query}, {"sceVideodecCreateDecoder",videodec_create},
    {"sceVideodecDecode",videodec_decode}, {"sceVideodecFlush",videodec_flush},
    {"sceVideodecReset",videodec_reset}, {"sceVideodecDeleteDecoder",videodec_delete},
};
uintptr_t runtime_videodec_resolve(const char *name) { return RUNTIME_LOOKUP(exports,name); }
