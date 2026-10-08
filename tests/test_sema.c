#define _GNU_SOURCE
#include "runtime.h"
#include <stdlib.h>

/* The settings menu of the GPU library restarts through probe.c, which tests do not link. */
void runtime_restart(void) { abort(); }
#include <assert.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <limits.h>
typedef int32_t (ABI *Create)(uint32_t *,const char *,uint32_t,int32_t,int32_t,const void *);
typedef int32_t (ABI *CountOp)(uint32_t,int32_t);
typedef int32_t (ABI *Wait)(uint32_t,int32_t,uint32_t *);
typedef int32_t (ABI *Delete)(uint32_t);
typedef int32_t (ABI *Cancel)(uint32_t,int32_t,int32_t *);
#define GET(t,n) ((t)runtime_resolve(n,0))
static Create create;
static CountOp poll_sem,signal_sem;
static Wait wait_sem;
static Delete delete_sem;
static Cancel cancel_sem;
static void setup(void) {
    runtime_start(1);
    create=GET(Create,"188x57JYp0g#libkernel");
    poll_sem=GET(CountOp,"12wOHk8ywb0#libkernel");
    signal_sem=GET(CountOp,"4czppHBiriw#libkernel");
    wait_sem=GET(Wait,"Zxa0VhQVTsk#libkernel");
    delete_sem=GET(Delete,"R1Jvn8bSCW8#libkernel");
    cancel_sem=GET(Cancel,"4DM06U2BNEY#libkernel");
    assert(create && poll_sem && signal_sem && wait_sem && delete_sem && cancel_sem);
    assert(!runtime_resolve("188x57JYp0g#libkernel",0));
    assert(!runtime_resolve("188x57JYp0g#libkernel",1));
}
static void lifecycle(void) {
    struct { uint32_t handle,canary; } s={0,0xdeadbeef};
    assert((uint32_t)create(NULL,"test",1,0,1,NULL)==0x80020016);
    assert((uint32_t)create(&s.handle,"test",1,2,1,NULL)==0x80020016);
    assert(create(&s.handle,"test",2,2,3,NULL)==0 && s.canary==0xdeadbeef);
    uint32_t old=s.handle,timeout=1000000;
    assert(wait_sem(s.handle,2,&timeout)==0 && timeout==1000000);
    assert((uint32_t)poll_sem(s.handle,1)==0x80020010);
    timeout=0;
    assert((uint32_t)wait_sem(s.handle,1,&timeout)==0x8002003c);
    assert((uint32_t)signal_sem(s.handle,4)==0x80020016);
    assert((uint32_t)poll_sem(s.handle,0)==0x80020016);
    assert((uint32_t)signal_sem(s.handle,-1)==0x80020016);
    assert(signal_sem(s.handle,3)==0);
    assert((uint32_t)signal_sem(s.handle,INT_MAX)==0x80020016);
    assert(poll_sem(s.handle,3)==0);
    int32_t n=-1;
    assert(cancel_sem(s.handle,-1,&n)==0 && n==0);
    assert(poll_sem(s.handle,2)==0);
    assert(delete_sem(s.handle)==0);
    assert((uint32_t)delete_sem(s.handle)==0x80020003);
    assert((uint32_t)signal_sem(s.handle,1)==0x80020003);
    assert(create(&s.handle,"test",1,0,1,NULL)==0 && s.handle!=old);
    assert((uint32_t)poll_sem(old,1)==0x80020003);
    timeout=1000;
    assert((uint32_t)wait_sem(s.handle,1,&timeout)==0x8002003c && timeout==0);
    assert(runtime_sema_waiters(s.handle)==0);
    assert(signal_sem(s.handle,1)==0 && poll_sem(s.handle,1)==0);
    assert(delete_sem(s.handle)==0);
}
typedef struct { uint32_t id; int32_t need,result; uint32_t timeout; } Task;
static void *waiter(void *p) { Task *t=p; t->result=wait_sem(t->id,t->need,&t->timeout); return NULL; }
static void enrolled(uint32_t id,unsigned count) {
    const struct timespec nap={0,1000000};
    for (int i=0;i<2000;++i) {
        if (runtime_sema_waiters(id)==count) return;
        nanosleep(&nap,NULL);
    }
    assert(!"waiter enrollment timed out");
}
static void concurrency(void) {
    uint32_t id;
    assert(create(&id,"fifo",1,0,3,NULL)==0);
    Task a={id,1,99,5000000},b={id,1,99,5000000};
    pthread_t first,second;
    assert(pthread_create(&first,NULL,waiter,&a)==0); enrolled(id,1);
    assert(pthread_create(&second,NULL,waiter,&b)==0); enrolled(id,2);
    assert(signal_sem(id,1)==0);
    assert(pthread_join(first,NULL)==0 && a.result==0 && a.timeout<=5000000);
    assert(runtime_sema_waiters(id)==1 && (uint32_t)poll_sem(id,1)==0x80020010);
    assert(signal_sem(id,1)==0);
    assert(pthread_join(second,NULL)==0 && b.result==0);
    /* A request that fits can pass an earlier larger request, matching the
       researched kernel semaphore model. No tokens are partially consumed. */
    a=(Task){id,2,99,5000000}; b=(Task){id,1,99,5000000};
    assert(pthread_create(&first,NULL,waiter,&a)==0); enrolled(id,1);
    assert(pthread_create(&second,NULL,waiter,&b)==0); enrolled(id,2);
    assert(signal_sem(id,1)==0);
    assert(pthread_join(second,NULL)==0 && b.result==0);
    assert(runtime_sema_waiters(id)==1);
    assert(signal_sem(id,2)==0);
    assert(pthread_join(first,NULL)==0 && a.result==0);
    assert(delete_sem(id)==0);
}
static void cancellation(void) {
    uint32_t id;
    assert(create(&id,"cancel",1,0,2,NULL)==0);
    Task a={id,1,99,5000000},b={id,1,99,5000000};
    pthread_t first,second;
    assert(pthread_create(&first,NULL,waiter,&a)==0); enrolled(id,1);
    assert(pthread_create(&second,NULL,waiter,&b)==0); enrolled(id,2);
    int32_t n=-1;
    assert(cancel_sem(id,1,&n)==0 && n==2);
    assert(pthread_join(first,NULL)==0 && (uint32_t)a.result==0x80020055);
    assert(pthread_join(second,NULL)==0 && (uint32_t)b.result==0x80020055);
    assert(a.timeout==0 && b.timeout==0 && poll_sem(id,1)==0);
    a=(Task){id,1,99,5000000}; b=(Task){id,1,99,5000000};
    assert(pthread_create(&first,NULL,waiter,&a)==0); enrolled(id,1);
    assert(pthread_create(&second,NULL,waiter,&b)==0); enrolled(id,2);
    assert(delete_sem(id)==0);
    assert(pthread_join(first,NULL)==0 && (uint32_t)a.result==0x8002000d);
    assert(pthread_join(second,NULL)==0 && (uint32_t)b.result==0x8002000d);
    assert((uint32_t)poll_sem(id,1)==0x80020003);
}
int main(int argc,char **argv) {
    setup();
    if (argc>1 && !strcmp(argv[1],"--priority")) {
        /* Priority semaphores block and are woken in FIFO order. */
        uint32_t id; assert(create(&id,"priority",2,0,1,NULL)==0);
        Task a={id,1,99,5000000}; pthread_t t;
        assert(pthread_create(&t,NULL,waiter,&a)==0); enrolled(id,1);
        assert(signal_sem(id,1)==0);
        assert(pthread_join(t,NULL)==0 && a.result==0);
        puts("PASS: priority semaphore wait"); return 0;
    }
    if (argc>1 && !strcmp(argv[1],"--concurrency")) concurrency();
    else if (argc>1 && !strcmp(argv[1],"--cancel-delete")) cancellation();
    else lifecycle();
    puts("PASS: semaphore contracts");
    return 0;
}
