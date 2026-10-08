/* libSceAjm: audio decoder batches. Batches use the SDK chunk format (the
 * builder functions are imports, but games may also build chunks inline), and
 * run synchronously at submission: decoding a batch takes well under a
 * millisecond, so waiting always finds it complete. ATRAC9 is decoded by
 * LibAtrac9 (MIT, third_party/LibAtrac9), MP3 by FFmpeg (runtime_mp3.c). AAC instances
 * stop explicitly. */
#define _GNU_SOURCE
#include "runtime.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <pthread.h>
#include "third_party/LibAtrac9/C/src/libatrac9.h"

#define ERR_INVALID_CONTEXT ((int32_t)0x80930002)
#define ERR_INVALID_INSTANCE ((int32_t)0x80930003)
#define ERR_INVALID_BATCH ((int32_t)0x80930004)
#define ERR_INVALID_PARAMETER ((int32_t)0x80930005)
#define ERR_OUT_OF_RESOURCES ((int32_t)0x80930007)
#define ERR_ALREADY_REGISTERED ((int32_t)0x80930009)
#define ERR_NOT_REGISTERED ((int32_t)0x8093000A)
#define ERR_WRONG_REVISION ((int32_t)0x8093000B)
#define ERR_MALFORMED_BATCH ((int32_t)0x80930011)
#define ERR_CANCELLED ((int32_t)0x80930017)
#define RESULT_NOT_INITIALIZED 0x1
#define RESULT_INVALID_PARAMETER 0x4
#define RESULT_PARTIAL_INPUT 0x8
#define RESULT_NOT_ENOUGH_ROOM 0x10
#define RESULT_CODEC_ERROR 0x40000000u
#define RESULT_FATAL 0x80000000u
#define STATISTICS_INSTANCE 0x80000u
#define MAX_INSTANCES 0x3fff
#define MAX_CONTEXTS 8
#define MAX_BATCHES 256
enum { IDENT_JOB=0, IDENT_INPUT_RUN=1, IDENT_INPUT_CONTROL=2, IDENT_CONTROL_FLAGS=3, IDENT_RUN_FLAGS=4,
       IDENT_RETURN_ADDRESS=6, IDENT_INLINE=7, IDENT_OUTPUT_RUN=17, IDENT_OUTPUT_CONTROL=18 };
enum { FORMAT_S16=0, FORMAT_S32=1, FORMAT_FLOAT=2 };
/* Job flags: version:3 codec:8 run:2 control:3 reserved:29 sideband:3 */
#define RUN_CODEC_INFO(f) (((f)>>11)&1)
#define RUN_MULTIPLE_FRAMES(f) (((f)>>12)&1)
#define CONTROL_RESET(f) (((f)>>13)&1)
#define CONTROL_INITIALIZE(f) (((f)>>14)&1)
#define CONTROL_RESAMPLE(f) (((f)>>15)&1)
#define SIDEBAND_GAPLESS(f) (((f)>>45)&1)
#define SIDEBAND_FORMAT(f) (((f)>>46)&1)
#define SIDEBAND_STREAM(f) (((f)>>47)&1)

typedef struct { uint32_t word; uint32_t size; } Chunk;          /* ident:6 payload:20 reserved:6 */
typedef struct { uint32_t word; uint32_t size; void *address; } ChunkBuffer;
typedef struct { int32_t result, internal_result; } SidebandResult;
typedef struct { int32_t input_consumed, output_written; uint64_t total_decoded_samples; } SidebandStream;
typedef struct { uint32_t channels, channel_mask, sample_rate, encoding, bitrate, reserved; } SidebandFormat;
typedef struct { uint32_t total_samples; uint16_t skip_samples, skipped_samples; } SidebandGapless;
typedef struct { uint32_t super_frame_size, frames_in_super_frame, next_frame_size, frame_samples; } At9Info;
typedef struct { int error_code; const void *job_address; uint32_t command_offset; const void *job_return_address; } BatchError;
_Static_assert(sizeof(ChunkBuffer)==16,"Ajm buffer chunk");
_Static_assert(sizeof(SidebandFormat)==24 && sizeof(SidebandGapless)==8 && sizeof(SidebandStream)==16,"Ajm sideband layout");

typedef struct {
    int used, codec, format, channels_hint, gapless_loop;
    uint32_t codec_flags;
    void *handle;
    unsigned char config[4];
    int initialized;
    Atrac9CodecInfo info;
    uint32_t superframe_remain, frames;
    SidebandGapless gapless_init, gapless;
    uint64_t total_samples;
    Mp3Decoder *mp3;
    uint32_t mp3_header, mp3_bitrate;
} Instance;
typedef struct { int used, registered[24]; Instance instances[MAX_INSTANCES+1]; } Context;
typedef struct { int used, context, canceled; } Batch;

static pthread_mutex_t lock=PTHREAD_RECURSIVE_MUTEX_INITIALIZER_NP;
static Context *contexts[MAX_CONTEXTS+1];
static Batch batches[MAX_BATCHES];
static size_t jobs_run, frames_decoded, batches_run;

/* BB_AUDIO_TRACE=1: instance configurations and jobs that return a non-zero result. */
static int trace_left=-1;
static int ajm_trace(void) {
    if (trace_left<0) { const char *e=getenv("BB_AUDIO_TRACE"); trace_left=e && e[0]=='1' ? 400 : 0; }
    return trace_left>0 && trace_left--;
}
static uint32_t ident(uint32_t word) { return word & 0x3f; }
static uint32_t payload(uint32_t word) { return (word>>6) & 0xfffff; }
static uint32_t header(uint32_t id, uint32_t value) { return (id & 0x3f) | ((value & 0xfffff)<<6); }
static uint32_t channel_mask(int channels) {
    switch (channels) { case 1: return 4; case 2: return 3; case 4: return 0x33; case 6: return 0x60f; case 8: return 0x63f; default: return 0; }
}
static int pcm_size(int format) { return format==FORMAT_S16 ? 2 : 4; }

/* ---- batch builders ---- */
static unsigned char *put_buffer(unsigned char *p, uint32_t id, const void *address, uint64_t size) {
    ChunkBuffer c={header(id,0),(uint32_t)size,(void *)address}; memcpy(p,&c,sizeof(c)); return p+sizeof(c);
}
static unsigned char *put_flags(unsigned char *p, uint32_t id, uint64_t flags) {
    Chunk c={header(id,(uint32_t)(flags>>32)),(uint32_t)flags}; memcpy(p,&c,sizeof(c)); return p+sizeof(c);
}
static void *finish_job(unsigned char *start, unsigned char *end, uint32_t instance) {
    Chunk job={header(IDENT_JOB,instance),(uint32_t)(end-start-sizeof(Chunk))};
    memcpy(start,&job,sizeof(job));
    return end;
}
static ABI void *job_control(void *buffer, uint32_t instance, uint64_t flags, void *in, uint64_t in_size,
                             void *out, uint64_t out_size, void *return_address) {
    unsigned char *p=(unsigned char *)buffer+sizeof(Chunk);
    if (return_address) p=put_buffer(p,IDENT_RETURN_ADDRESS,return_address,0);
    p=put_buffer(p,IDENT_INPUT_CONTROL,in,in_size);
    flags &= instance==STATISTICS_INSTANCE ? UINT64_C(0xC0018007) : UINT64_C(0x60000000E7FF);
    p=put_flags(p,IDENT_CONTROL_FLAGS,flags);
    p=put_buffer(p,IDENT_OUTPUT_CONTROL,out,out_size);
    return finish_job(buffer,p,instance);
}
static ABI void *job_run(void *buffer, uint32_t instance, uint64_t flags, void *in, uint64_t in_size,
                         void *out, uint64_t out_size, void *sideband, uint64_t sideband_size, void *return_address) {
    unsigned char *p=(unsigned char *)buffer+sizeof(Chunk);
    if (return_address) p=put_buffer(p,IDENT_RETURN_ADDRESS,return_address,0);
    p=put_buffer(p,IDENT_INPUT_RUN,in,in_size);
    p=put_flags(p,IDENT_RUN_FLAGS,flags & UINT64_C(0xE00000001FFF));
    p=put_buffer(p,IDENT_OUTPUT_RUN,out,out_size);
    p=put_buffer(p,IDENT_OUTPUT_CONTROL,sideband,sideband_size);
    return finish_job(buffer,p,instance);
}
typedef struct { void *address; uint64_t size; } AjmBuffer;
static ABI void *job_run_split(void *buffer, uint32_t instance, uint64_t flags, const AjmBuffer *in, uint64_t in_count,
                               const AjmBuffer *out, uint64_t out_count, void *sideband, uint64_t sideband_size,
                               void *return_address) {
    unsigned char *p=(unsigned char *)buffer+sizeof(Chunk);
    if (return_address) p=put_buffer(p,IDENT_RETURN_ADDRESS,return_address,0);
    for (uint64_t i=0;i<in_count;++i) p=put_buffer(p,IDENT_INPUT_RUN,in[i].address,in[i].size);
    p=put_flags(p,IDENT_RUN_FLAGS,flags & UINT64_C(0xE00000001FFF));
    for (uint64_t i=0;i<out_count;++i) p=put_buffer(p,IDENT_OUTPUT_RUN,out[i].address,out[i].size);
    p=put_buffer(p,IDENT_OUTPUT_CONTROL,sideband,sideband_size);
    return finish_job(buffer,p,instance);
}
static ABI void *job_inline(void *buffer, const void *data, uint64_t size, const void **address) {
    Chunk c={header(IDENT_INLINE,0),(uint32_t)((size+7)&~UINT64_C(7))};
    memcpy(buffer,&c,sizeof(c));
    unsigned char *p=(unsigned char *)buffer+sizeof(c);
    memcpy(p,data,size);
    if (address) *address=p;
    return p+c.size;
}

/* ---- ATRAC9 decoding ---- */
static void at9_reset(Instance *in) {
    if (in->handle) Atrac9ReleaseHandle(in->handle);
    in->handle=Atrac9GetHandle();
    in->initialized = Atrac9InitDecoder(in->handle,in->config)==0;
    if (in->initialized) Atrac9GetCodecInfo(in->handle,&in->info);
    in->frames=0; in->superframe_remain=(uint32_t)in->info.superframeSize;
    if (ajm_trace())
        printf("Audio trace: ATRAC9 init %s: %d ch, %d Hz, frame %d samples, %d frames/superframe, superframe %d bytes, format %d, flags %#x\n",
               in->initialized ? "ok" : "FAILED",in->info.channels,in->info.samplingRate,in->info.frameSamples,
               in->info.framesInSuperframe,in->info.superframeSize,in->format,in->codec_flags);
}
static void gapless_reset(Instance *in) {
    in->gapless.total_samples=in->gapless_init.total_samples;
    in->gapless.skip_samples=in->gapless_init.skip_samples;
    in->gapless.skipped_samples=0;
}
static int gapless_end(const Instance *in) { return in->gapless_init.total_samples && !in->gapless.total_samples; }
static uint32_t next_frame_bytes(const Instance *in) {
    uint32_t frame=(uint32_t)in->info.frameSamples;
    uint32_t skip=in->gapless.skip_samples<frame ? in->gapless.skip_samples : frame;
    uint32_t samples=frame-skip;
    if (in->gapless_init.total_samples && in->gapless.total_samples<samples) samples=in->gapless.total_samples;
    return samples*(uint32_t)in->info.channels*(uint32_t)pcm_size(in->format);
}
typedef struct { ChunkBuffer *chunks; int count, current; uint32_t offset; } Output;
static uint64_t output_room(const Output *o) {
    uint64_t room=0;
    for (int i=o->current;i<o->count;++i) room+=o->chunks[i].size-(i==o->current ? o->offset : 0);
    return room;
}
static void output_write(Output *o, const unsigned char *data, uint64_t size) {
    while (size && o->current<o->count) {
        ChunkBuffer *c=&o->chunks[o->current];
        uint64_t n=c->size-o->offset; if (n>size) n=size;
        memcpy((unsigned char *)c->address+o->offset,data,n);
        data+=n; size-=n; o->offset+=(uint32_t)n;
        if (o->offset==c->size) { ++o->current; o->offset=0; }
    }
}
/* One frame; returns result flags. */
static uint32_t at9_frame(Instance *in, const unsigned char **input, uint64_t *input_size, Output *out,
                          uint32_t *samples_written, int32_t *internal) {
    int channels=in->info.channels, frame=in->info.frameSamples, used=0;
    int16_t pcm[256*8];
    if (frame*channels>(int)(sizeof(pcm)/sizeof(*pcm))) return RESULT_CODEC_ERROR|RESULT_FATAL;
    int e=Atrac9Decode(in->handle,*input,pcm,&used);
    if (e) { *internal=e; return RESULT_CODEC_ERROR|RESULT_FATAL; }
    *input+=used; *input_size-=(uint64_t)used; in->superframe_remain-=(uint32_t)used;
    ++frames_decoded;
    uint32_t skip=0;
    if (in->gapless.skip_samples) {
        skip=in->gapless.skip_samples<(uint32_t)frame ? in->gapless.skip_samples : (uint32_t)frame;
        in->gapless.skip_samples-=(uint16_t)skip;
    }
    uint32_t samples=(uint32_t)frame-skip;
    if (in->gapless_init.total_samples && samples>in->gapless.total_samples) samples=in->gapless.total_samples;
    uint64_t room=output_room(out)/(uint64_t)(channels*pcm_size(in->format));
    if (samples>room) samples=(uint32_t)room;
    /* Convert (and optionally de-interleave) into the requested encoding. */
    unsigned char converted[256*8*4];
    int non_interleaved=(in->codec_flags>>8)&1;
    for (uint32_t s=0;s<samples;++s) for (int c=0;c<channels;++c) {
        int16_t v=pcm[(skip+s)*(uint32_t)channels+(uint32_t)c];
        size_t index=non_interleaved ? (size_t)c*samples+s : (size_t)s*(size_t)channels+(size_t)c;
        if (in->format==FORMAT_S16) memcpy(converted+index*2,&v,2);
        else if (in->format==FORMAT_S32) { int32_t w=(int32_t)v<<16; memcpy(converted+index*4,&w,4); }
        else { float f=(float)v/32768.0f; memcpy(converted+index*4,&f,4); }
    }
    output_write(out,converted,(uint64_t)samples*(uint64_t)channels*(uint64_t)pcm_size(in->format));
    *samples_written=samples;
    in->gapless.skipped_samples+=(uint16_t)((uint32_t)frame-samples);
    if (in->gapless_init.total_samples) in->gapless.total_samples-=samples;
    if (++in->frames%(uint32_t)in->info.framesInSuperframe==0) {
        uint64_t skip_bytes=in->superframe_remain<*input_size ? in->superframe_remain : *input_size;
        *input+=skip_bytes; *input_size-=skip_bytes;
        in->superframe_remain=(uint32_t)in->info.superframeSize; in->frames=0;
    } else if (gapless_end(in)) {
        /* Drain the rest of the superframe so the next stream starts aligned. */
        while (in->frames%(uint32_t)in->info.framesInSuperframe && *input_size) {
            if (Atrac9Decode(in->handle,*input,pcm,&used)) break;
            *input+=used; *input_size-=(uint64_t)used; in->superframe_remain-=(uint32_t)used; ++in->frames;
        }
        uint64_t skip_bytes=in->superframe_remain<*input_size ? in->superframe_remain : *input_size;
        *input+=skip_bytes; *input_size-=skip_bytes;
        in->superframe_remain=(uint32_t)in->info.superframeSize; in->frames=0;
    }
    return 0;
}
static void parse_riff(Instance *in, const unsigned char **input, uint64_t *size) {
    const unsigned char *p=*input, *end=p+*size;
    p+=12;
    while (p+8<=end) {
        uint32_t tag, length; memcpy(&tag,p,4); memcpy(&length,p+4,4); p+=8;
        if (tag==0x61746164) break;                                   /* "data" */
        if (tag==0x20746d66 && length>=52 && p+52<=end) {             /* "fmt " */
            memcpy(in->config,p+44,4); at9_reset(in);
        } else if (tag==0x74636166 && length>=12 && p+12<=end) {      /* "fact" */
            uint32_t total, delay; memcpy(&total,p,4); memcpy(&delay,p+4,4);
            in->gapless_init.total_samples=total; in->gapless_init.skip_samples=(uint16_t)delay;
            gapless_reset(in);
        }
        p+=length;
    }
    *size-=(uint64_t)(p-*input); *input=p;
}

/* ---- MP3 decoding ---- */
enum { CODEC_MP3=0, CODEC_AT9=1 };
static void instance_release(Instance *in) {
    if (in->handle) Atrac9ReleaseHandle(in->handle);
    runtime_mp3_close(in->mp3);
}
/* The frame at the input: its header sets the frame layout (and the gapless metadata of a tag
 * frame) before the room and input checks. */
static uint32_t mp3_next(Instance *in, const unsigned char *input, uint64_t input_size) {
    if (input_size<4) return RESULT_PARTIAL_INPUT;
    Mp3Frame f;
    int ignore_ofl=in->codec_flags & 1;
    if (runtime_mp3_parse(input,input_size>UINT32_MAX ? UINT32_MAX : (uint32_t)input_size,!ignore_ofl,1,&f))
        return RESULT_CODEC_ERROR|RESULT_FATAL;
    in->info.channels=(int)f.num_channels; in->info.samplingRate=(int)f.sample_rate;
    in->info.frameSamples=(int)f.samples_per_channel; in->info.framesInSuperframe=1;
    in->info.superframeSize=(int)f.frame_size; in->superframe_remain=(uint32_t)f.frame_size;
    in->mp3_header=(uint32_t)input[0]<<24|(uint32_t)input[1]<<16|(uint32_t)input[2]<<8|input[3];
    in->mp3_bitrate=f.bitrate;
    if (f.total_samples || f.encoder_delay) {
        in->gapless_init.total_samples=f.total_samples; in->gapless_init.skip_samples=(uint16_t)f.encoder_delay;
        gapless_reset(in);
    }
    return 0;
}
static uint32_t mp3_frame(Instance *in, const unsigned char **input, uint64_t *input_size, Output *out,
                          uint32_t *samples_written, int32_t *internal) {
    float pcm[1152*2];
    int channels=in->info.channels;
    uint32_t size=in->superframe_remain;
    int frame=runtime_mp3_decode(in->mp3,*input,(int)size,pcm,1152,&channels);
    *input+=size; *input_size-=size; in->superframe_remain=0;
    if (frame<0 || channels!=in->info.channels) { *internal=-1; return RESULT_CODEC_ERROR|RESULT_FATAL; }
    ++frames_decoded;
    uint32_t skip=in->gapless.skip_samples<(uint32_t)frame ? in->gapless.skip_samples : (uint32_t)frame;
    in->gapless.skip_samples-=(uint16_t)skip;
    uint32_t samples=(uint32_t)frame-skip;
    if (in->gapless_init.total_samples && samples>in->gapless.total_samples) samples=in->gapless.total_samples;
    uint64_t room=output_room(out)/(uint64_t)(channels*pcm_size(in->format));
    if (samples>room) samples=(uint32_t)room;
    unsigned char converted[1152*2*4];
    for (uint32_t i=0;i<samples*(uint32_t)channels;++i) {
        float v=pcm[skip*(uint32_t)channels+i];
        v=v>1.0f ? 1.0f : v<-1.0f ? -1.0f : v;
        if (in->format==FORMAT_S16) { int16_t w=(int16_t)(v*32767.0f); memcpy(converted+i*2,&w,2); }
        else if (in->format==FORMAT_S32) { int32_t w=(int32_t)(v*2147483647.0); memcpy(converted+i*4,&w,4); }
        else memcpy(converted+i*4,&v,4);
    }
    output_write(out,converted,(uint64_t)samples*(uint64_t)channels*(uint64_t)pcm_size(in->format));
    *samples_written=samples;
    in->gapless.skipped_samples+=(uint16_t)((uint32_t)frame-samples);
    if (in->gapless_init.total_samples) in->gapless.total_samples-=samples;
    return 0;
}
static ABI int32_t ajm_mp3_parse_frame(const unsigned char *data, uint32_t size, int parse_ofl, Mp3Frame *frame) {
    return runtime_mp3_parse(data,size,parse_ofl,1,frame) ? ERR_INVALID_PARAMETER : 0;
}

/* ---- job execution ---- */
typedef struct {
    uint64_t flags; int have_flags;
    ChunkBuffer inputs[16]; int input_count;
    ChunkBuffer outputs[16]; int output_count;
    ChunkBuffer input_control, output_control;
} Job;
static int parse_job(const unsigned char *p, uint32_t size, Job *job) {
    memset(job,0,sizeof(*job));
    for (const unsigned char *end=p+size; p<end;) {
        uint32_t word; memcpy(&word,p,4);
        switch (ident(word)) {
        case IDENT_CONTROL_FLAGS: case IDENT_RUN_FLAGS: {
            Chunk c; memcpy(&c,p,8);
            job->flags=((uint64_t)payload(c.word)<<32)|c.size; job->have_flags=1; p+=8; break;
        }
        case IDENT_INPUT_RUN: if (job->input_count==16) return -1; memcpy(&job->inputs[job->input_count++],p,16); p+=16; break;
        case IDENT_OUTPUT_RUN: if (job->output_count==16) return -1; memcpy(&job->outputs[job->output_count++],p,16); p+=16; break;
        case IDENT_INPUT_CONTROL: memcpy(&job->input_control,p,16); p+=16; break;
        case IDENT_OUTPUT_CONTROL: memcpy(&job->output_control,p,16); p+=16; break;
        case IDENT_RETURN_ADDRESS: p+=16; break;
        default: return -1;
        }
    }
    return job->have_flags ? 0 : -1;
}
static void run_job(Context *ctx, uint32_t id, Job *job) {
    ++jobs_run;
    unsigned char *side=job->output_control.size ? job->output_control.address : NULL;
    unsigned char *side_end=side ? side+job->output_control.size : NULL;
    SidebandResult *result=NULL;
    if (side && side+sizeof(SidebandResult)<=side_end) { result=(void *)side; side+=sizeof(*result); memset(result,0,sizeof(*result)); }
    if (id==STATISTICS_INSTANCE) { if (side) memset(side,0,(size_t)(side_end-side)); return; }
    Instance *in=&ctx->instances[id & MAX_INSTANCES];
    if ((id & MAX_INSTANCES)==0 || !in->used) { if (result) result->result=(int32_t)(RESULT_FATAL|RESULT_INVALID_PARAMETER); return; }
    uint64_t f=job->flags;
    uint32_t flags_result=0;
    const unsigned char *control=job->input_control.size ? job->input_control.address : NULL;
    const unsigned char *control_end=control ? control+job->input_control.size : NULL;
    if (CONTROL_RESET(f)) {
        in->total_samples=0; gapless_reset(in);
        if (in->codec==CODEC_MP3) runtime_mp3_reset(in->mp3);
        else if (in->initialized) at9_reset(in);
    }
    if (control && SIDEBAND_FORMAT(f) && control+24<=control_end) control+=24; /* output format fixed per instance */
    if (control && SIDEBAND_GAPLESS(f) && control+8<=control_end) {
        SidebandGapless g; memcpy(&g,control,8); control+=8;
        uint32_t processed=in->gapless_init.total_samples-in->gapless.total_samples;
        if (g.total_samples || !g.skip_samples) {
            if (g.total_samples>=processed) {
                int64_t diff=(int64_t)in->gapless_init.total_samples-g.total_samples;
                in->gapless_init.total_samples=g.total_samples; in->gapless.total_samples=(uint32_t)(in->gapless.total_samples-diff);
            } else flags_result|=RESULT_INVALID_PARAMETER;
        }
        uint32_t skipped=(uint32_t)in->gapless_init.skip_samples-in->gapless.skip_samples;
        if (g.skip_samples || !g.total_samples) {
            if (g.skip_samples>=skipped) {
                int32_t diff=(int32_t)in->gapless_init.skip_samples-g.skip_samples;
                in->gapless_init.skip_samples=g.skip_samples; in->gapless.skip_samples=(uint16_t)(in->gapless.skip_samples-diff);
            } else flags_result|=RESULT_INVALID_PARAMETER;
        }
    }
    if (control && CONTROL_RESAMPLE(f) && control+8<=control_end) control+=8;
    if (control && CONTROL_INITIALIZE(f) && control+8<=control_end && in->codec==CODEC_AT9) { memcpy(in->config,control,4); at9_reset(in); }
    uint64_t in_size=0;
    for (int i=0;i<job->input_count;++i) in_size+=job->inputs[i].size;
    unsigned char *joined=NULL;
    const unsigned char *input=NULL;
    if (job->input_count==1) input=job->inputs[0].address;
    else if (job->input_count>1) {
        joined=malloc(in_size ? in_size : 1);
        uint64_t at=0;
        for (int i=0;i<job->input_count;++i) { memcpy(joined+at,job->inputs[i].address,job->inputs[i].size); at+=job->inputs[i].size; }
        input=joined;
    }
    Output out={job->outputs,job->output_count,0,0};
    uint64_t in_start=in_size, out_start=output_room(&out);
    uint32_t frames=0;
    int32_t internal=0;
    if (in_size) for (;;) {
        if (in->gapless_loop && gapless_end(in)) { gapless_reset(in); in->total_samples=0; }
        if (in->codec==CODEC_AT9 && (in->codec_flags & 1) && in_size>=4 && !memcmp(input,"RIFF",4)) { parse_riff(in,&input,&in_size); in->total_samples=0; }
        if (in->codec==CODEC_MP3 && (flags_result|=mp3_next(in,input,in_size))) break;
        if (!in->initialized) { flags_result|=RESULT_NOT_INITIALIZED; break; }
        if (!gapless_end(in) && output_room(&out)<next_frame_bytes(in)) flags_result|=RESULT_NOT_ENOUGH_ROOM;
        if (in_size<in->superframe_remain) flags_result|=RESULT_PARTIAL_INPUT;
        if (flags_result) break;
        uint32_t written=0;
        uint32_t r=in->codec==CODEC_MP3 ? mp3_frame(in,&input,&in_size,&out,&written,&internal)
                                        : at9_frame(in,&input,&in_size,&out,&written,&internal);
        in->total_samples+=written; ++frames;
        if (r) { flags_result|=r; break; }
        if (!RUN_MULTIPLE_FRAMES(f)) break;
    }
    free(joined);
    if (flags_result && ajm_trace())
        printf("Audio trace: Ajm job instance %u flags %#llx -> result %#x internal %d; input %llu/%llu used, output %llu/%llu written, frames %u\n",
               id,(unsigned long long)f,flags_result,internal,(unsigned long long)(in_start-in_size),(unsigned long long)in_start,
               (unsigned long long)(out_start-output_room(&out)),(unsigned long long)out_start,frames);
    if (result) { result->result=(int32_t)flags_result; result->internal_result=internal; }
    /* Output sideband order: stream, format, gapless, multi-frame, codec info. */
    if (side && SIDEBAND_STREAM(f) && side+16<=side_end) {
        SidebandStream s={(int32_t)(in_start-in_size),(int32_t)(out_start-output_room(&out)),in->total_samples};
        memcpy(side,&s,16); side+=16;
    }
    if (side && SIDEBAND_FORMAT(f) && side+24<=side_end) {
        SidebandFormat s={(uint32_t)in->info.channels,channel_mask(in->info.channels),(uint32_t)in->info.samplingRate,
                          (uint32_t)in->format,0,0};
        if (in->codec==CODEC_MP3) s.bitrate=in->mp3_bitrate;
        else if (in->info.framesInSuperframe && in->info.frameSamples)
            s.bitrate=(uint32_t)((uint64_t)in->info.samplingRate*(uint64_t)in->info.superframeSize*8/
                                 ((uint64_t)in->info.framesInSuperframe*(uint64_t)in->info.frameSamples));
        memcpy(side,&s,24); side+=24;
    }
    if (side && SIDEBAND_GAPLESS(f) && side+8<=side_end) { memcpy(side,&in->gapless,8); side+=8; }
    if (side && RUN_MULTIPLE_FRAMES(f) && side+8<=side_end) { uint32_t m[2]={frames,0}; memcpy(side,m,8); side+=8; }
    if (side && RUN_CODEC_INFO(f) && side+16<=side_end && in->codec==CODEC_MP3) {
        /* AjmSidebandDecMp3CodecInfo: header, has_crc, channel_mode, mode_extension, copyright, original, emphasis. */
        uint32_t h=in->mp3_header;
        unsigned char info[16]={0};
        memcpy(info,&h,4);
        info[4]=!((h>>16)&1); info[5]=(h>>6)&3; info[6]=(h>>4)&3; info[7]=(h>>3)&1; info[8]=(h>>2)&1; info[9]=h&3;
        memcpy(side,info,16);
    } else if (side && RUN_CODEC_INFO(f) && side+16<=side_end) {
        At9Info info={(uint32_t)in->info.superframeSize,(uint32_t)in->info.framesInSuperframe,in->superframe_remain,(uint32_t)in->info.frameSamples};
        memcpy(side,&info,16);
    }
}

/* ---- API ---- */
static Context *context(uint32_t id) { return id && id<=MAX_CONTEXTS ? contexts[id] : NULL; }
static ABI int32_t ajm_initialize(int64_t reserved, uint32_t *out) {
    if (!out || reserved) return ERR_INVALID_PARAMETER;
    pthread_mutex_lock(&lock);
    for (uint32_t i=1;i<=MAX_CONTEXTS;++i) if (!contexts[i]) {
        contexts[i]=calloc(1,sizeof(Context));
        pthread_mutex_unlock(&lock);
        if (!contexts[i]) return ERR_OUT_OF_RESOURCES;
        *out=i; puts("Runtime: Ajm context initialized (ATRAC9 via LibAtrac9, MP3 via FFmpeg)");
        return 0;
    }
    pthread_mutex_unlock(&lock);
    return ERR_OUT_OF_RESOURCES;
}
static ABI int32_t ajm_finalize(uint32_t id) {
    pthread_mutex_lock(&lock);
    Context *c=context(id);
    if (!c) { pthread_mutex_unlock(&lock); return ERR_INVALID_CONTEXT; }
    for (int i=0;i<=MAX_INSTANCES;++i) instance_release(&c->instances[i]);
    free(c); contexts[id]=NULL;
    pthread_mutex_unlock(&lock);
    return 0;
}
static ABI int32_t ajm_module_register(uint32_t id, uint32_t codec, int64_t reserved) {
    if (reserved || codec>=24) return ERR_INVALID_PARAMETER;
    pthread_mutex_lock(&lock);
    Context *c=context(id);
    int32_t r=!c ? ERR_INVALID_CONTEXT : c->registered[codec] ? ERR_ALREADY_REGISTERED : 0;
    if (!r) c->registered[codec]=1;
    pthread_mutex_unlock(&lock);
    return r;
}
static ABI int32_t ajm_module_unregister(uint32_t id, uint32_t codec) {
    pthread_mutex_lock(&lock);
    Context *c=context(id);
    int32_t r=!c ? ERR_INVALID_CONTEXT : codec>=24 ? ERR_INVALID_PARAMETER : !c->registered[codec] ? ERR_NOT_REGISTERED : 0;
    if (!r) c->registered[codec]=0;
    pthread_mutex_unlock(&lock);
    return r;
}
static ABI int32_t ajm_instance_create(uint32_t id, uint32_t codec, uint64_t flags, uint32_t *out) {
    if (!out || codec>=24) return ERR_INVALID_PARAMETER;
    if (!(flags & 7)) return ERR_WRONG_REVISION;
    if (codec!=CODEC_MP3 && codec!=CODEC_AT9) { fprintf(stderr,"STOP: Ajm codec %u (2=AAC) is not implemented\n",codec); exit(21); }
    if (ajm_trace()) printf("Audio trace: Ajm instance create codec %u flags %#llx\n",codec,(unsigned long long)flags);
    pthread_mutex_lock(&lock);
    Context *c=context(id);
    int32_t r=!c ? ERR_INVALID_CONTEXT : !c->registered[codec] ? ERR_NOT_REGISTERED : ERR_OUT_OF_RESOURCES;
    if (c && c->registered[codec]) for (uint32_t i=1;i<=MAX_INSTANCES;++i) if (!c->instances[i].used) {
        Instance *in=&c->instances[i];
        memset(in,0,sizeof(*in));
        in->used=1; in->codec=(int)codec; in->channels_hint=(int)((flags>>3)&15); in->format=(int)((flags>>7)&7);
        in->gapless_loop=(int)((flags>>10)&1); in->codec_flags=(uint32_t)(flags>>32);
        if (in->format>FORMAT_FLOAT) { in->used=0; r=ERR_INVALID_PARAMETER; break; }
        if (codec==CODEC_MP3) {
            in->mp3=runtime_mp3_open();
            if (!in->mp3) { in->used=0; r=ERR_OUT_OF_RESOURCES; break; }
            in->initialized=1;
        }
        *out=i|(codec<<14); r=0; break;
    }
    pthread_mutex_unlock(&lock);
    return r;
}
static ABI int32_t ajm_instance_destroy(uint32_t id, uint32_t instance) {
    pthread_mutex_lock(&lock);
    Context *c=context(id);
    Instance *in=c ? &c->instances[instance & MAX_INSTANCES] : NULL;
    int32_t r=!c ? ERR_INVALID_CONTEXT : !(instance & MAX_INSTANCES) || !in->used ? ERR_INVALID_INSTANCE : 0;
    if (!r) { instance_release(in); memset(in,0,sizeof(*in)); }
    pthread_mutex_unlock(&lock);
    return r;
}
static ABI uint32_t ajm_instance_codec(uint32_t instance) { return (instance>>14)&0x1f; }
static ABI int32_t ajm_batch_start(uint32_t id, unsigned char *buffer, uint32_t size, int priority,
                                   BatchError *error, uint32_t *out) {
    (void)priority;
    if (size & 7) return ERR_MALFORMED_BATCH;
    pthread_mutex_lock(&lock);
    Context *c=context(id);
    if (!c) { pthread_mutex_unlock(&lock); return ERR_INVALID_CONTEXT; }
    int slot=-1;
    for (int i=0;i<MAX_BATCHES;++i) if (!batches[i].used) { slot=i; break; }
    if (slot<0) { pthread_mutex_unlock(&lock); return ERR_OUT_OF_RESOURCES; }
    for (unsigned char *p=buffer, *end=buffer+size; p<end;) {
        Chunk chunk; memcpy(&chunk,p,8);
        unsigned char *body=p+8;
        p=body+chunk.size;
        if (ident(chunk.word)==IDENT_INLINE) continue;
        Job job;
        if (ident(chunk.word)!=IDENT_JOB || p>end || parse_job(body,chunk.size,&job)) {
            if (error) { error->error_code=ERR_MALFORMED_BATCH; error->job_address=body-8; error->command_offset=(uint32_t)(body-8-buffer); error->job_return_address=NULL; }
            pthread_mutex_unlock(&lock);
            return ERR_MALFORMED_BATCH;
        }
        run_job(c,payload(chunk.word),&job);
    }
    batches[slot]=(Batch){1,(int)id,0};
    ++batches_run;
    *out=(uint32_t)slot+1;
    pthread_mutex_unlock(&lock);
    return 0;
}
static ABI int32_t ajm_batch_wait(uint32_t id, uint32_t batch, uint32_t timeout, BatchError *error) {
    (void)timeout; (void)error;
    pthread_mutex_lock(&lock);
    int32_t r=!context(id) ? ERR_INVALID_CONTEXT : !batch || batch>MAX_BATCHES || !batches[batch-1].used ? ERR_INVALID_BATCH : 0;
    if (!r) { if (batches[batch-1].canceled) r=ERR_CANCELLED; batches[batch-1].used=0; }
    pthread_mutex_unlock(&lock);
    return r;
}
static ABI int32_t ajm_batch_cancel(uint32_t id, uint32_t batch) {
    pthread_mutex_lock(&lock);
    int32_t r=!context(id) ? ERR_INVALID_CONTEXT : !batch || batch>MAX_BATCHES || !batches[batch-1].used ? ERR_INVALID_BATCH : 0;
    pthread_mutex_unlock(&lock);
    return r; /* already processed at submission: nothing to cancel */
}
static ABI int32_t ajm_error_dump(void) { return 0; }
static ABI int32_t ajm_memory_register(uint32_t id, void *p, uint64_t pages) { (void)p; (void)pages; return context(id) ? 0 : ERR_INVALID_CONTEXT; }

static const RuntimeExport exports[]={
    {"sceAjmInitialize",ajm_initialize}, {"sceAjmFinalize",ajm_finalize},
    {"sceAjmModuleRegister",ajm_module_register}, {"sceAjmModuleUnregister",ajm_module_unregister},
    {"sceAjmInstanceCreate",ajm_instance_create}, {"sceAjmInstanceDestroy",ajm_instance_destroy},
    {"sceAjmInstanceCodecType",ajm_instance_codec},
    {"sceAjmBatchJobControlBufferRa",job_control}, {"sceAjmBatchJobRunBufferRa",job_run},
    {"sceAjmBatchJobRunSplitBufferRa",job_run_split}, {"sceAjmBatchJobInlineBuffer",job_inline},
    {"sceAjmBatchStartBuffer",ajm_batch_start}, {"sceAjmBatchWait",ajm_batch_wait},
    {"sceAjmBatchCancel",ajm_batch_cancel}, {"sceAjmBatchErrorDump",ajm_error_dump},
    {"sceAjmMemoryRegister",ajm_memory_register}, {"sceAjmDecMp3ParseFrame",ajm_mp3_parse_frame},
};
uintptr_t runtime_ajm_resolve(const char *name) { return RUNTIME_LOOKUP(exports,name); }
void runtime_ajm_report(void) {
    printf("Runtime: Ajm batches=%zu, jobs=%zu, ATRAC9 frames decoded=%zu\n",batches_run,jobs_run,frames_decoded);
}
