/* MP3 for libSceAjm (runtime_ajm.c): frame header parsing with the gapless metadata the
 * SDK reads (LAME/Xing, VBRI, FGH), and frame decoding by FFmpeg. Header tables and the
 * metadata layouts follow shadPS4's ajm_mp3.cpp (GPL-2.0-or-later). */
#include "runtime.h"
#include <stdlib.h>
#include <string.h>
#include <libavcodec/avcodec.h>

/* [version][index]: version 0=MPEG 2.5, 1 reserved, 2=MPEG 2, 3=MPEG 1. */
static const int32_t sample_rates[4][4]={{11025,12000,8000,0},{0,0,0,0},{22050,24000,16000,0},{44100,48000,32000,0}};
static const int32_t bitrates[4][16]={
    {0,8,16,24,32,40,48,56,64,0,0,0,0,0,0,0},{0},
    {0,8,16,24,32,40,48,56,64,80,96,112,128,144,160,0},
    {0,32,40,48,56,64,80,96,112,128,160,192,224,256,320,0}};
enum { OFL_NONE=0, OFL_LAME=1, OFL_VBRI=2, OFL_FGH=3, OFL_VBRI_FGH=4 };

static uint32_t be32(const unsigned char *p) { return (uint32_t)p[0]<<24|(uint32_t)p[1]<<16|(uint32_t)p[2]<<8|p[3]; }
static uint32_t bits(uint32_t word, int shift, int count) { return (word>>shift)&((1u<<count)-1); }

typedef struct { const unsigned char *data; size_t size, bit; } BitReader;
static uint32_t read_bits(BitReader *r, int count) {
    uint32_t v=0;
    for (int i=0;i<count;++i,++r->bit)
        v=v<<1|(r->bit/8<r->size ? (r->data[r->bit/8]>>(7-r->bit%8))&1 : 0);
    return v;
}

/* Gapless metadata after the side information of the first frame. */
static void parse_extended(const unsigned char *begin, uint32_t size, uint32_t header, Mp3Frame *f) {
    const unsigned char *end=begin+(size<f->frame_size ? size : f->frame_size), *p=begin+4;
    if (p>=end) return;
    int v1=bits(header,19,2)==3, mono=bits(header,6,2)==3;
    BitReader r={p,(size_t)(end-p),0};
    if (!bits(header,16,1)) r.bit+=16;                     /* CRC */
    r.bit+=v1 ? (mono ? 18 : 20) : (mono ? 9 : 10);
    uint32_t part23=0;
    for (int gr=0;gr<(v1 ? 2 : 1);++gr) for (uint32_t ch=0;ch<f->num_channels;++ch) {
        part23+=read_bits(&r,12);
        r.bit+=v1 ? 47 : 51;
    }
    r.bit+=part23;
    size_t offset=(r.bit+7)/8;
    p=offset>r.size ? end : p+offset;
    if (p+8<=end && (!memcmp(p,"Xing",4) || !memcmp(p,"Info",4))) {
        unsigned flags=p[7];
        const unsigned char *field=p+8;
        uint32_t frames=0;
        if (flags&1) { if (field+4>end) return; frames=be32(field); f->num_frames=frames; field+=4; }
        if (flags&2) field+=4;
        if (flags&4) field+=100;
        if (flags&8) field+=4;
        if (field+0x18<=end && !memcmp(field,"LAME",4)) {
            uint32_t delay=(uint32_t)field[0x15]<<4|field[0x16]>>4, padding=(uint32_t)(field[0x16]&15)<<8|field[0x17];
            if ((flags&1) && frames) f->total_samples=frames*f->samples_per_channel-(delay+padding);
            /* The tag frame itself decodes to silence; 529 is the decoder delay. */
            f->encoder_delay=f->samples_per_channel+delay+529;
            f->ofl_type=OFL_LAME;
        }
    } else if (p+26<=end && !memcmp(p,"VBRI",4)) {
        f->encoder_delay=(uint32_t)p[6]<<8|p[7];
        f->ofl_type=OFL_VBRI;
        if (f->frame_size<=size) {
            Mp3Frame next;
            if (!runtime_mp3_parse(begin+f->frame_size,size-(uint32_t)f->frame_size,1,0,&next) && next.ofl_type==OFL_FGH) {
                f->encoder_delay+=next.encoder_delay; f->total_samples=next.total_samples; f->ofl_type=OFL_VBRI_FGH;
            }
        }
    } else {
        while (p+9<end && *p!=0xB4) ++p;                    /* FGH indicator */
        if (p+9>=end) return;
        uint8_t crc=0xFF;
        for (int i=0;i<9;++i) for (int j=7;j>=0;--j)
            crc=(uint8_t)(((crc>>7)&1)!=((p[i]>>j)&1) ? crc*2 : (crc*2)^0x45);
        if (p[9]!=crc) return;
        f->encoder_delay=(uint32_t)p[1]<<8|p[2];
        f->total_samples=be32(p+3);
        f->ofl_type=OFL_FGH;
    }
}

int runtime_mp3_parse(const unsigned char *data, uint32_t size, int parse_ofl, int layer3_only, Mp3Frame *f) {
    if (!data || size<4 || !f) return -1;
    uint32_t h=be32(data);
    uint32_t version=bits(h,19,2);
    if (bits(h,21,11)!=0x7FF || (layer3_only && bits(h,17,2)!=1)) return -1;
    memset(f,0,sizeof(*f));
    f->sample_rate=(uint32_t)sample_rates[version][bits(h,10,2)];
    f->bitrate=(uint32_t)bitrates[version][bits(h,12,4)]*1000;
    if (!f->sample_rate || !f->bitrate) return -1;
    f->num_channels=bits(h,6,2)==3 ? 1 : 2;
    f->samples_per_channel=version==3 ? 1152 : 576;
    f->frame_size=(version==3 ? 144 : 72)*f->bitrate/f->sample_rate+bits(h,9,1);
    if (parse_ofl) parse_extended(data,size,h,f);
    return 0;
}

struct Mp3Decoder { AVCodecContext *codec; AVPacket *packet; AVFrame *frame; };

Mp3Decoder *runtime_mp3_open(void) {
    const AVCodec *codec=avcodec_find_decoder(AV_CODEC_ID_MP3);
    Mp3Decoder *d=calloc(1,sizeof(*d));
    if (!codec || !d) { free(d); return NULL; }
    d->codec=avcodec_alloc_context3(codec);
    d->packet=av_packet_alloc();
    d->frame=av_frame_alloc();
    if (!d->codec || !d->packet || !d->frame || avcodec_open2(d->codec,codec,NULL)<0) { runtime_mp3_close(d); return NULL; }
    return d;
}
void runtime_mp3_close(Mp3Decoder *d) {
    if (!d) return;
    av_frame_free(&d->frame);
    av_packet_free(&d->packet);
    avcodec_free_context(&d->codec);
    free(d);
}
void runtime_mp3_reset(Mp3Decoder *d) { if (d) avcodec_flush_buffers(d->codec); }

/* One whole frame in, interleaved float out (max samples*channels floats). Returns samples per
 * channel (0 while the decoder buffers), or -1 for a codec error. */
int runtime_mp3_decode(Mp3Decoder *d, const unsigned char *data, int size, float *pcm, int max_samples, int *channels) {
    d->packet->data=(uint8_t *)data;
    d->packet->size=size;
    if (avcodec_send_packet(d->codec,d->packet)<0) return -1;
    int written=0;
    for (;;) {
        int r=avcodec_receive_frame(d->codec,d->frame);
        if (r==AVERROR(EAGAIN) || r==AVERROR_EOF) break;
        if (r<0) return -1;
        int ch=d->frame->ch_layout.nb_channels, n=d->frame->nb_samples;
        if (ch<1 || ch>2 || written+n>max_samples) { av_frame_unref(d->frame); return -1; }
        enum AVSampleFormat fmt=(enum AVSampleFormat)d->frame->format;
        for (int s=0;s<n;++s) for (int c=0;c<ch;++c) {
            float v;
            if (fmt==AV_SAMPLE_FMT_FLTP) v=((const float *)d->frame->extended_data[c])[s];
            else if (fmt==AV_SAMPLE_FMT_FLT) v=((const float *)d->frame->data[0])[s*ch+c];
            else if (fmt==AV_SAMPLE_FMT_S16P) v=((const int16_t *)d->frame->extended_data[c])[s]/32768.0f;
            else if (fmt==AV_SAMPLE_FMT_S16) v=((const int16_t *)d->frame->data[0])[s*ch+c]/32768.0f;
            else { av_frame_unref(d->frame); return -1; }
            pcm[(written+s)*ch+c]=v;
        }
        written+=n; *channels=ch;
        av_frame_unref(d->frame);
    }
    return written;
}
