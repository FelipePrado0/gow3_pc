/* MP3 through libSceAjm: header/gapless parsing and decoding jobs (tests/data/sine.mp3:
 * 0.5 s of 440 Hz, 44.1 kHz stereo, made with ffmpeg -c:a libmp3lame -id3v2_version 0; the
 * encoder field of its tag frame is renamed from "Lavc63.1" to "LAME3.10", the only one the
 * SDK reads). */
#undef NDEBUG /* the checks are asserts; release builds define NDEBUG */
#include "../src/runtime_ajm.c"
#include <assert.h>

uintptr_t runtime_lookup(const RuntimeExport *table, size_t count, const char *nid) { (void)table; (void)count; (void)nid; return 0; }

static unsigned char *load(const char *path, uint32_t *size) {
    FILE *f=fopen(path,"rb");
    assert(f);
    static unsigned char data[1<<16];
    *size=(uint32_t)fread(data,1,sizeof(data),f);
    fclose(f);
    return data;
}

static uint64_t frame_size_of(const unsigned char *data) {
    Mp3Frame f; assert(runtime_mp3_parse(data,4,0,1,&f)==0); return f.frame_size;
}

/* One decoding job; returns the sideband result and fills stream/gapless. */
static int32_t run(uint32_t ctx, uint32_t instance, const void *in, uint64_t in_size, void *out, uint64_t out_size,
                   uint64_t flags, SidebandStream *stream, SidebandGapless *gapless) {
    unsigned char batch[256], side[64]={0};
    unsigned char *end=job_run(batch,instance,flags,(void *)in,in_size,out,out_size,side,sizeof(side),NULL);
    uint32_t id;
    assert(ajm_batch_start(ctx,batch,(uint32_t)(end-batch),0,NULL,&id)==0);
    assert(ajm_batch_wait(ctx,id,0,NULL)==0);
    SidebandResult r; memcpy(&r,side,8);
    if (stream) memcpy(stream,side+8,16);
    if (gapless) memcpy(gapless,side+24,8);
    return r.result;
}

int main(int argc, char **argv) {
    uint32_t size;
    const unsigned char *mp3=load(argc>1 ? argv[1] : "tests/data/sine.mp3",&size);

    Mp3Frame frame;
    assert(ajm_mp3_parse_frame(mp3,size,1,&frame)==0);
    assert(frame.sample_rate==44100 && frame.num_channels==2 && frame.samples_per_channel==1152);
    assert(frame.ofl_type==1 && frame.total_samples==22050 && frame.encoder_delay>1152);
    assert(ajm_mp3_parse_frame((const unsigned char *)"\0\0\0\0",4,1,&frame)==ERR_INVALID_PARAMETER);
    assert(ajm_mp3_parse_frame(mp3,3,1,&frame)==ERR_INVALID_PARAMETER);

    uint32_t ctx, instance;
    assert(ajm_initialize(0,&ctx)==0 && ajm_module_register(ctx,0,0)==0);
    assert(ajm_instance_create(ctx,0,1|(FORMAT_S16<<7),&instance)==0 && ajm_instance_codec(instance)==0);

    /* The whole stream in one multi-frame job: exactly the gapless samples come out. */
    static int16_t pcm[44100*2];
    SidebandStream stream; SidebandGapless gapless;
    uint64_t multi=UINT64_C(1)<<12|UINT64_C(1)<<47|UINT64_C(1)<<45;
    int32_t r=run(ctx,instance,mp3,size,pcm,sizeof(pcm),multi,&stream,&gapless);
    assert((r & ~RESULT_PARTIAL_INPUT)==0);
    assert(stream.output_written==22050*2*2 && stream.total_decoded_samples==22050);
    assert(stream.input_consumed>0 && (uint32_t)stream.input_consumed<=size);
    int16_t peak=0;
    for (int i=0;i<22050*2;++i) if (pcm[i]>peak) peak=pcm[i];
    /* Reference: ffmpeg -i sine.mp3 -f s16le gives 22050 samples, peak 2761, [0]=10, [1000]=-196
     * (gapless trimming at the same sample; +-1 for float to s16 rounding). */
    assert(peak>=2759 && peak<=2763 && abs(pcm[0]-10)<=1 && abs(pcm[1000]+196)<=1);

    /* Reset, then a small output: the skipped tag frame fits, the next frame does not. */
    unsigned char control[8];
    unsigned char batch[128];
    unsigned char *end=job_control(batch,instance,UINT64_C(1)<<13,control,0,control,sizeof(control),NULL);
    uint32_t id; assert(ajm_batch_start(ctx,batch,(uint32_t)(end-batch),0,NULL,&id)==0 && ajm_batch_wait(ctx,id,0,NULL)==0);
    int16_t small[16];
    r=run(ctx,instance,mp3,size,small,sizeof(small),multi,&stream,NULL);
    assert(r==RESULT_NOT_ENOUGH_ROOM && stream.output_written==0 && stream.input_consumed==(int32_t)frame_size_of(mp3));

    /* Garbage is a codec error result, not a crash. */
    static const unsigned char junk[512]={0x12,0x34};
    r=run(ctx,instance,junk,sizeof(junk),pcm,sizeof(pcm),UINT64_C(1)<<47,&stream,NULL);
    assert(r & RESULT_CODEC_ERROR);

    assert(ajm_instance_destroy(ctx,instance)==0 && ajm_finalize(ctx)==0);
    puts("mp3 test: ok");
    return 0;
}
