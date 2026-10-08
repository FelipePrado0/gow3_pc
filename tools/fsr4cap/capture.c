// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: D3D12 call recorder for fsr4cap.exe. Patches the vtables of the device, command list
// and resource classes (vkd3d-proton shares one vtable per class across interface versions) and
// writes every compute dispatch with its pipeline, root parameters and the descriptors they
// resolve to into capture/trace.txt; shaders (DXIL), root signatures and buffer uploads go to
// capture/ as files.
#define COBJMACROS
#define WIDL_C_INLINE_WRAPPERS
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <d3d12.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "capture.h"

static FILE* trace;
static char dir[MAX_PATH];
static int dispatch_count;

static uint64_t Fnv(const void* data, size_t size) {
    uint64_t h = 1469598103934665603ull;
    for (size_t i = 0; i < size; ++i) h = (h ^ ((const uint8_t*)data)[i]) * 1099511628211ull;
    return h;
}

static void WriteFile2(const char* name, const void* data, size_t size) {
    char path[MAX_PATH];
    snprintf(path, sizeof(path), "%s\\%s", dir, name);
    FILE* f = fopen(path, "rb");
    if (f) { fclose(f); return; } // content-addressed: already there
    f = fopen(path, "wb");
    if (f) { fwrite(data, 1, size, f); fclose(f); }
}

// ---- resources ---------------------------------------------------------------------------
typedef struct { ID3D12Resource* res; int id; D3D12_RESOURCE_DESC desc; D3D12_HEAP_TYPE heap;
                 D3D12_GPU_VIRTUAL_ADDRESS va; } ResInfo;
static ResInfo resources[4096];
static int resource_count;

typedef D3D12_GPU_VIRTUAL_ADDRESS (STDMETHODCALLTYPE* PfnGetGpuVa)(ID3D12Resource*);
static void* real_get_va;

static ResInfo* Res(ID3D12Resource* r) {
    if (!r) return NULL;
    for (int i = 0; i < resource_count; ++i) if (resources[i].res == r) return &resources[i];
    if (resource_count == 4096) return NULL;
    ResInfo* info = &resources[resource_count];
    info->res = r;
    info->id = resource_count++;
    info->desc = ID3D12Resource_GetDesc(r);
    D3D12_HEAP_PROPERTIES props = {0};
    D3D12_HEAP_FLAGS flags;
    info->heap = SUCCEEDED(ID3D12Resource_GetHeapProperties(r, &props, &flags)) ? props.Type : 0;
    info->va = info->desc.Dimension == D3D12_RESOURCE_DIMENSION_BUFFER
                   ? (real_get_va ? ((PfnGetGpuVa)real_get_va)(r) : ID3D12Resource_GetGPUVirtualAddress(r)) : 0;
    fprintf(trace, "RESOURCE r%d dim=%d %llux%ux%u mips=%u format=%d flags=0x%x heap=%d va=0x%llx\n",
            info->id, info->desc.Dimension, (unsigned long long)info->desc.Width, info->desc.Height,
            info->desc.DepthOrArraySize, info->desc.MipLevels, info->desc.Format, info->desc.Flags,
            info->heap, (unsigned long long)info->va);
    return info;
}

static ResInfo* ResByVa(D3D12_GPU_VIRTUAL_ADDRESS va, uint64_t* offset) {
    for (int i = 0; i < resource_count; ++i) {
        ResInfo* r = &resources[i];
        if (r->va && va >= r->va && va < r->va + r->desc.Width) { *offset = va - r->va; return r; }
    }
    return NULL;
}

// Reads `size` bytes of an upload/readback buffer at `offset` (CPU visible), or null.
static const void* MapRead(ResInfo* r, uint64_t offset, uint64_t size) {
    if (!r || (r->heap != D3D12_HEAP_TYPE_UPLOAD && r->heap != D3D12_HEAP_TYPE_READBACK)) return NULL;
    if (offset >= r->desc.Width) return NULL;
    void* data = NULL;
    if (FAILED(ID3D12Resource_Map(r->res, 0, NULL, &data)) || !data) return NULL;
    ID3D12Resource_Unmap(r->res, 0, NULL);
    return (const uint8_t*)data + offset;
}

static void DumpBytes(const char* what, ResInfo* r, uint64_t offset, uint64_t size) {
    if (r && r->desc.Dimension == D3D12_RESOURCE_DIMENSION_BUFFER && offset < r->desc.Width && offset + size > r->desc.Width)
        size = r->desc.Width - offset; // a footprint's last row is shorter than its pitch
    const void* data = MapRead(r, offset, size);
    if (!data) { fprintf(trace, " %s=(not CPU visible)", what); return; }
    char name[64];
    const uint64_t h = Fnv(data, size);
    snprintf(name, sizeof(name), "data_%016llx.bin", (unsigned long long)h);
    WriteFile2(name, data, size);
    fprintf(trace, " %s=%s", what, name);
}

// ---- descriptors ---------------------------------------------------------------------------
typedef struct { int kind; ResInfo* res; uint64_t a, b; int format; char text[160]; } View;
enum { VIEW_NONE, VIEW_CBV, VIEW_SRV, VIEW_UAV, VIEW_SAMPLER };
typedef struct { SIZE_T cpu; UINT64 gpu; UINT count, inc; View* views; } Heap;
static Heap heaps[64];
static int heap_count;

static View* ViewAtCpu(SIZE_T cpu) {
    for (int i = 0; i < heap_count; ++i) {
        Heap* h = &heaps[i];
        if (cpu >= h->cpu && cpu < h->cpu + (SIZE_T)h->count * h->inc) return &h->views[(cpu - h->cpu) / h->inc];
    }
    return NULL;
}

static View* ViewAtGpu(UINT64 gpu, Heap** heap) {
    for (int i = 0; i < heap_count; ++i) {
        Heap* h = &heaps[i];
        if (h->gpu && gpu >= h->gpu && gpu < h->gpu + (UINT64)h->count * h->inc) {
            *heap = h;
            return &h->views[(gpu - h->gpu) / h->inc];
        }
    }
    return NULL;
}

// ---- pipelines and root signatures ----------------------------------------------------------
typedef struct { void* obj; uint64_t hash; void* root; } Pso;
static Pso psos[1024];
static int pso_count;
typedef struct { void* obj; uint64_t hash; } Root;
static Root roots[256];
static int root_count;

static uint64_t RootHash(void* obj) {
    for (int i = 0; i < root_count; ++i) if (roots[i].obj == obj) return roots[i].hash;
    return 0;
}

// ---- command list state ---------------------------------------------------------------------
typedef struct { int type; UINT64 value; UINT32 consts[64]; UINT nconsts; } RootArg;
enum { ARG_NONE, ARG_TABLE, ARG_CBV, ARG_SRV, ARG_UAV, ARG_CONSTS };
typedef struct { void* list; void* root; void* pso; RootArg args[32]; } ListState;
static ListState lists[16];

static ListState* State(void* list) {
    for (int i = 0; i < 16; ++i) if (lists[i].list == list) return &lists[i];
    for (int i = 0; i < 16; ++i) if (!lists[i].list) { lists[i].list = list; return &lists[i]; }
    return &lists[0];
}

// ---- hooks ------------------------------------------------------------------------------------
#define SLOT(vtbl, method) (offsetof(vtbl, method) / sizeof(void*))
static void** device_vtbl;
static void** list_vtbl;
static void* real[512];
#define REAL(fn_type, slot) ((fn_type)real[slot])

static void Patch(void** vtbl, size_t slot, void* hook, int store) {
    DWORD old;
    VirtualProtect(&vtbl[slot], sizeof(void*), PAGE_READWRITE, &old);
    real[store] = vtbl[slot];
    vtbl[slot] = hook;
    VirtualProtect(&vtbl[slot], sizeof(void*), old, &old);
}

// Device hooks use store slots 0..99, command list 100..299, resource 300..
typedef HRESULT (STDMETHODCALLTYPE* PfnCreateComputePipelineState)(ID3D12Device*, const D3D12_COMPUTE_PIPELINE_STATE_DESC*, REFIID, void**);
static HRESULT STDMETHODCALLTYPE HookCreateComputePipelineState(ID3D12Device* This, const D3D12_COMPUTE_PIPELINE_STATE_DESC* desc, REFIID riid, void** out) {
    HRESULT hr = REAL(PfnCreateComputePipelineState, 0)(This, desc, riid, out);
    if (SUCCEEDED(hr) && out && *out && pso_count < 1024) {
        const uint64_t h = Fnv(desc->CS.pShaderBytecode, desc->CS.BytecodeLength);
        char name[64];
        snprintf(name, sizeof(name), "cs_%016llx.dxil", (unsigned long long)h);
        WriteFile2(name, desc->CS.pShaderBytecode, desc->CS.BytecodeLength);
        psos[pso_count++] = (Pso){*out, h, desc->pRootSignature};
        fprintf(trace, "PSO %p cs=%016llx root=%016llx\n", *out, (unsigned long long)h,
                (unsigned long long)RootHash(desc->pRootSignature));
    }
    return hr;
}

typedef HRESULT (STDMETHODCALLTYPE* PfnCreateRootSignature)(ID3D12Device*, UINT, const void*, SIZE_T, REFIID, void**);
static HRESULT STDMETHODCALLTYPE HookCreateRootSignature(ID3D12Device* This, UINT mask, const void* blob, SIZE_T size, REFIID riid, void** out) {
    HRESULT hr = REAL(PfnCreateRootSignature, 1)(This, mask, blob, size, riid, out);
    if (SUCCEEDED(hr) && out && *out && root_count < 256) {
        const uint64_t h = Fnv(blob, size);
        char name[64];
        snprintf(name, sizeof(name), "root_%016llx.bin", (unsigned long long)h);
        WriteFile2(name, blob, size);
        roots[root_count++] = (Root){*out, h};
        fprintf(trace, "ROOTSIG %p %016llx\n", *out, (unsigned long long)h);
        CaptureDescribeRootSignature(trace, blob, size);
    }
    return hr;
}

typedef HRESULT (STDMETHODCALLTYPE* PfnCreateDescriptorHeap)(ID3D12Device*, const D3D12_DESCRIPTOR_HEAP_DESC*, REFIID, void**);
static HRESULT STDMETHODCALLTYPE HookCreateDescriptorHeap(ID3D12Device* This, const D3D12_DESCRIPTOR_HEAP_DESC* desc, REFIID riid, void** out) {
    HRESULT hr = REAL(PfnCreateDescriptorHeap, 2)(This, desc, riid, out);
    if (SUCCEEDED(hr) && out && *out && heap_count < 64) {
        ID3D12DescriptorHeap* h = *out;
        Heap* heap = &heaps[heap_count++];
        heap->cpu = ID3D12DescriptorHeap_GetCPUDescriptorHandleForHeapStart(h).ptr;
        heap->gpu = (desc->Flags & D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE)
                        ? ID3D12DescriptorHeap_GetGPUDescriptorHandleForHeapStart(h).ptr : 0;
        heap->count = desc->NumDescriptors;
        heap->inc = ID3D12Device_GetDescriptorHandleIncrementSize(This, desc->Type);
        heap->views = calloc(desc->NumDescriptors, sizeof(View));
        fprintf(trace, "HEAP type=%d count=%u visible=%d\n", desc->Type, desc->NumDescriptors, heap->gpu != 0);
    }
    return hr;
}

typedef void (STDMETHODCALLTYPE* PfnCreateCBV)(ID3D12Device*, const D3D12_CONSTANT_BUFFER_VIEW_DESC*, D3D12_CPU_DESCRIPTOR_HANDLE);
static void STDMETHODCALLTYPE HookCreateCBV(ID3D12Device* This, const D3D12_CONSTANT_BUFFER_VIEW_DESC* desc, D3D12_CPU_DESCRIPTOR_HANDLE h) {
    REAL(PfnCreateCBV, 3)(This, desc, h);
    View* v = ViewAtCpu(h.ptr);
    if (!v || !desc) return;
    uint64_t off = 0;
    *v = (View){VIEW_CBV, ResByVa(desc->BufferLocation, &off), off, desc->SizeInBytes, 0, ""};
    snprintf(v->text, sizeof(v->text), "CBV r%d+%llu size %u", v->res ? v->res->id : -1, (unsigned long long)off, desc->SizeInBytes);
}

typedef void (STDMETHODCALLTYPE* PfnCreateSRV)(ID3D12Device*, ID3D12Resource*, const D3D12_SHADER_RESOURCE_VIEW_DESC*, D3D12_CPU_DESCRIPTOR_HANDLE);
static void STDMETHODCALLTYPE HookCreateSRV(ID3D12Device* This, ID3D12Resource* res, const D3D12_SHADER_RESOURCE_VIEW_DESC* desc, D3D12_CPU_DESCRIPTOR_HANDLE h) {
    REAL(PfnCreateSRV, 4)(This, res, desc, h);
    View* v = ViewAtCpu(h.ptr);
    if (!v) return;
    ResInfo* r = Res(res);
    *v = (View){VIEW_SRV, r, 0, 0, desc ? desc->Format : -1, ""};
    if (desc && desc->ViewDimension == D3D12_SRV_DIMENSION_BUFFER)
        snprintf(v->text, sizeof(v->text), "SRV r%d buffer first=%llu num=%u stride=%u format=%d flags=%d", r ? r->id : -1,
                 (unsigned long long)desc->Buffer.FirstElement, desc->Buffer.NumElements, desc->Buffer.StructureByteStride, desc->Format, desc->Buffer.Flags);
    else
        snprintf(v->text, sizeof(v->text), "SRV r%d dim=%d format=%d", r ? r->id : -1, desc ? desc->ViewDimension : -1, desc ? desc->Format : -1);
}

typedef void (STDMETHODCALLTYPE* PfnCreateUAV)(ID3D12Device*, ID3D12Resource*, ID3D12Resource*, const D3D12_UNORDERED_ACCESS_VIEW_DESC*, D3D12_CPU_DESCRIPTOR_HANDLE);
static void STDMETHODCALLTYPE HookCreateUAV(ID3D12Device* This, ID3D12Resource* res, ID3D12Resource* counter, const D3D12_UNORDERED_ACCESS_VIEW_DESC* desc, D3D12_CPU_DESCRIPTOR_HANDLE h) {
    REAL(PfnCreateUAV, 5)(This, res, counter, desc, h);
    View* v = ViewAtCpu(h.ptr);
    if (!v) return;
    ResInfo* r = Res(res);
    *v = (View){VIEW_UAV, r, 0, 0, desc ? desc->Format : -1, ""};
    if (desc && desc->ViewDimension == D3D12_UAV_DIMENSION_BUFFER)
        snprintf(v->text, sizeof(v->text), "UAV r%d buffer first=%llu num=%u stride=%u format=%d flags=%d", r ? r->id : -1,
                 (unsigned long long)desc->Buffer.FirstElement, desc->Buffer.NumElements, desc->Buffer.StructureByteStride, desc->Format, desc->Buffer.Flags);
    else
        snprintf(v->text, sizeof(v->text), "UAV r%d dim=%d format=%d mip=%u", r ? r->id : -1, desc ? desc->ViewDimension : -1,
                 desc ? desc->Format : -1, desc && desc->ViewDimension == D3D12_UAV_DIMENSION_TEXTURE2D ? desc->Texture2D.MipSlice : 0);
}

typedef void (STDMETHODCALLTYPE* PfnCreateSampler)(ID3D12Device*, const D3D12_SAMPLER_DESC*, D3D12_CPU_DESCRIPTOR_HANDLE);
static void STDMETHODCALLTYPE HookCreateSampler(ID3D12Device* This, const D3D12_SAMPLER_DESC* desc, D3D12_CPU_DESCRIPTOR_HANDLE h) {
    REAL(PfnCreateSampler, 6)(This, desc, h);
    View* v = ViewAtCpu(h.ptr);
    if (!v || !desc) return;
    *v = (View){VIEW_SAMPLER, NULL, 0, 0, 0, ""};
    snprintf(v->text, sizeof(v->text), "SAMPLER filter=0x%x address=%d,%d,%d", desc->Filter, desc->AddressU, desc->AddressV, desc->AddressW);
}

typedef void (STDMETHODCALLTYPE* PfnCopyDescriptorsSimple)(ID3D12Device*, UINT, D3D12_CPU_DESCRIPTOR_HANDLE, D3D12_CPU_DESCRIPTOR_HANDLE, D3D12_DESCRIPTOR_HEAP_TYPE);
static void STDMETHODCALLTYPE HookCopyDescriptorsSimple(ID3D12Device* This, UINT n, D3D12_CPU_DESCRIPTOR_HANDLE dst, D3D12_CPU_DESCRIPTOR_HANDLE src, D3D12_DESCRIPTOR_HEAP_TYPE type) {
    REAL(PfnCopyDescriptorsSimple, 7)(This, n, dst, src, type);
    const UINT inc = ID3D12Device_GetDescriptorHandleIncrementSize(This, type);
    for (UINT i = 0; i < n; ++i) {
        View* s = ViewAtCpu(src.ptr + (SIZE_T)i * inc);
        View* d = ViewAtCpu(dst.ptr + (SIZE_T)i * inc);
        if (s && d) *d = *s;
    }
}

typedef void (STDMETHODCALLTYPE* PfnCopyDescriptors)(ID3D12Device*, UINT, const D3D12_CPU_DESCRIPTOR_HANDLE*, const UINT*, UINT, const D3D12_CPU_DESCRIPTOR_HANDLE*, const UINT*, D3D12_DESCRIPTOR_HEAP_TYPE);
static void STDMETHODCALLTYPE HookCopyDescriptors(ID3D12Device* This, UINT nd, const D3D12_CPU_DESCRIPTOR_HANDLE* dsts, const UINT* dsizes, UINT ns, const D3D12_CPU_DESCRIPTOR_HANDLE* srcs, const UINT* ssizes, D3D12_DESCRIPTOR_HEAP_TYPE type) {
    REAL(PfnCopyDescriptors, 8)(This, nd, dsts, dsizes, ns, srcs, ssizes, type);
    const UINT inc = ID3D12Device_GetDescriptorHandleIncrementSize(This, type);
    UINT di = 0, dk = 0;
    for (UINT si = 0; si < ns; ++si) {
        const UINT scount = ssizes ? ssizes[si] : 1;
        for (UINT k = 0; k < scount && di < nd; ++k) {
            View* s = ViewAtCpu(srcs[si].ptr + (SIZE_T)k * inc);
            View* d = ViewAtCpu(dsts[di].ptr + (SIZE_T)dk * inc);
            if (s && d) *d = *s;
            if (++dk == (dsizes ? dsizes[di] : 1)) { dk = 0; ++di; }
        }
    }
}

// Command list.
typedef void (STDMETHODCALLTYPE* PfnSetPtr)(ID3D12GraphicsCommandList*, void*);
static void STDMETHODCALLTYPE HookSetComputeRootSignature(ID3D12GraphicsCommandList* This, void* rs) {
    State(This)->root = rs;
    memset(State(This)->args, 0, sizeof(State(This)->args));
    REAL(PfnSetPtr, 100)(This, rs);
}
static void STDMETHODCALLTYPE HookSetPipelineState(ID3D12GraphicsCommandList* This, void* pso) {
    State(This)->pso = pso;
    REAL(PfnSetPtr, 101)(This, pso);
}
typedef void (STDMETHODCALLTYPE* PfnSetTable)(ID3D12GraphicsCommandList*, UINT, D3D12_GPU_DESCRIPTOR_HANDLE);
static void STDMETHODCALLTYPE HookSetComputeRootDescriptorTable(ID3D12GraphicsCommandList* This, UINT i, D3D12_GPU_DESCRIPTOR_HANDLE h) {
    if (i < 32) State(This)->args[i] = (RootArg){ARG_TABLE, h.ptr};
    REAL(PfnSetTable, 102)(This, i, h);
}
typedef void (STDMETHODCALLTYPE* PfnSet32)(ID3D12GraphicsCommandList*, UINT, UINT, UINT);
static void STDMETHODCALLTYPE HookSetComputeRoot32BitConstant(ID3D12GraphicsCommandList* This, UINT i, UINT v, UINT off) {
    if (i < 32 && off < 64) {
        RootArg* a = &State(This)->args[i];
        a->type = ARG_CONSTS;
        a->consts[off] = v;
        if (off + 1 > a->nconsts) a->nconsts = off + 1;
    }
    REAL(PfnSet32, 103)(This, i, v, off);
}
typedef void (STDMETHODCALLTYPE* PfnSet32s)(ID3D12GraphicsCommandList*, UINT, UINT, const void*, UINT);
static void STDMETHODCALLTYPE HookSetComputeRoot32BitConstants(ID3D12GraphicsCommandList* This, UINT i, UINT n, const void* v, UINT off) {
    if (i < 32 && off + n <= 64) {
        RootArg* a = &State(This)->args[i];
        a->type = ARG_CONSTS;
        memcpy(&a->consts[off], v, n * 4);
        if (off + n > a->nconsts) a->nconsts = off + n;
    }
    REAL(PfnSet32s, 104)(This, i, n, v, off);
}
typedef void (STDMETHODCALLTYPE* PfnSetVa)(ID3D12GraphicsCommandList*, UINT, D3D12_GPU_VIRTUAL_ADDRESS);
static void STDMETHODCALLTYPE HookSetComputeRootCBV(ID3D12GraphicsCommandList* This, UINT i, D3D12_GPU_VIRTUAL_ADDRESS va) {
    if (i < 32) State(This)->args[i] = (RootArg){ARG_CBV, va};
    REAL(PfnSetVa, 105)(This, i, va);
}
static void STDMETHODCALLTYPE HookSetComputeRootSRV(ID3D12GraphicsCommandList* This, UINT i, D3D12_GPU_VIRTUAL_ADDRESS va) {
    if (i < 32) State(This)->args[i] = (RootArg){ARG_SRV, va};
    REAL(PfnSetVa, 106)(This, i, va);
}
static void STDMETHODCALLTYPE HookSetComputeRootUAV(ID3D12GraphicsCommandList* This, UINT i, D3D12_GPU_VIRTUAL_ADDRESS va) {
    if (i < 32) State(This)->args[i] = (RootArg){ARG_UAV, va};
    REAL(PfnSetVa, 107)(This, i, va);
}

static uint64_t PsoHash(void* pso) {
    for (int i = 0; i < pso_count; ++i) if (psos[i].obj == pso) return psos[i].hash;
    return 0;
}

typedef void (STDMETHODCALLTYPE* PfnDispatch)(ID3D12GraphicsCommandList*, UINT, UINT, UINT);
static void STDMETHODCALLTYPE HookDispatch(ID3D12GraphicsCommandList* This, UINT x, UINT y, UINT z) {
    ListState* s = State(This);
    fprintf(trace, "DISPATCH #%d cs=%016llx root=%016llx groups=%u,%u,%u\n", dispatch_count++,
            (unsigned long long)PsoHash(s->pso), (unsigned long long)RootHash(s->root), x, y, z);
    for (int i = 0; i < 32; ++i) {
        RootArg* a = &s->args[i];
        if (a->type == ARG_NONE) continue;
        fprintf(trace, "  param %d:", i);
        if (a->type == ARG_CONSTS) {
            for (UINT k = 0; k < a->nconsts; ++k) fprintf(trace, " %08x", a->consts[k]);
        } else if (a->type == ARG_TABLE) {
            Heap* heap = NULL;
            View* v = ViewAtGpu(a->value, &heap);
            fprintf(trace, " table@%llu", heap ? (unsigned long long)((a->value - heap->gpu) / heap->inc) : 0ull);
            // The table's size comes from the root signature; print the next 24 non-empty slots.
            for (int k = 0; v && k < 24 && (v + k) < heap->views + heap->count; ++k) {
                if (v[k].kind == VIEW_NONE) continue;
                fprintf(trace, "\n    [%d] %s", k, v[k].text);
                if (v[k].kind == VIEW_CBV) DumpBytes("data", v[k].res, v[k].a, v[k].b);
            }
        } else {
            uint64_t off = 0;
            ResInfo* r = ResByVa(a->value, &off);
            fprintf(trace, " %s r%d+%llu", a->type == ARG_CBV ? "CBV" : a->type == ARG_SRV ? "SRV" : "UAV",
                    r ? r->id : -1, (unsigned long long)off);
            if (a->type == ARG_CBV && r) {
                const uint64_t size = r->desc.Width - off < 4096 ? r->desc.Width - off : 4096;
                DumpBytes("data", r, off, size);
            }
        }
        fprintf(trace, "\n");
    }
    REAL(PfnDispatch, 108)(This, x, y, z);
}

typedef void (STDMETHODCALLTYPE* PfnCopyBufferRegion)(ID3D12GraphicsCommandList*, ID3D12Resource*, UINT64, ID3D12Resource*, UINT64, UINT64);
static void STDMETHODCALLTYPE HookCopyBufferRegion(ID3D12GraphicsCommandList* This, ID3D12Resource* dst, UINT64 doff, ID3D12Resource* src, UINT64 soff, UINT64 n) {
    ResInfo* d = Res(dst);
    ResInfo* s = Res(src);
    fprintf(trace, "COPYBUFFER r%d+%llu <- r%d+%llu size %llu", d ? d->id : -1, (unsigned long long)doff,
            s ? s->id : -1, (unsigned long long)soff, (unsigned long long)n);
    DumpBytes("data", s, soff, n);
    fprintf(trace, "\n");
    REAL(PfnCopyBufferRegion, 109)(This, dst, doff, src, soff, n);
}

typedef void (STDMETHODCALLTYPE* PfnCopyResource)(ID3D12GraphicsCommandList*, ID3D12Resource*, ID3D12Resource*);
static void STDMETHODCALLTYPE HookCopyResource(ID3D12GraphicsCommandList* This, ID3D12Resource* dst, ID3D12Resource* src) {
    ResInfo* d = Res(dst);
    ResInfo* s = Res(src);
    fprintf(trace, "COPYRESOURCE r%d <- r%d", d ? d->id : -1, s ? s->id : -1);
    if (s && s->desc.Dimension == D3D12_RESOURCE_DIMENSION_BUFFER) DumpBytes("data", s, 0, s->desc.Width);
    fprintf(trace, "\n");
    REAL(PfnCopyResource, 110)(This, dst, src);
}

typedef void (STDMETHODCALLTYPE* PfnCopyTextureRegion)(ID3D12GraphicsCommandList*, const D3D12_TEXTURE_COPY_LOCATION*, UINT, UINT, UINT, const D3D12_TEXTURE_COPY_LOCATION*, const D3D12_BOX*);
static void STDMETHODCALLTYPE HookCopyTextureRegion(ID3D12GraphicsCommandList* This, const D3D12_TEXTURE_COPY_LOCATION* dst, UINT x, UINT y, UINT z, const D3D12_TEXTURE_COPY_LOCATION* src, const D3D12_BOX* box) {
    ResInfo* d = Res(dst->pResource);
    ResInfo* s = Res(src->pResource);
    fprintf(trace, "COPYTEXTURE r%d (type %d) <- r%d (type %d)", d ? d->id : -1, dst->Type, s ? s->id : -1, src->Type);
    if (src->Type == D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT && s) {
        const D3D12_PLACED_SUBRESOURCE_FOOTPRINT* f = &src->PlacedFootprint;
        fprintf(trace, " footprint offset %llu %ux%u format %d pitch %u", (unsigned long long)f->Offset,
                f->Footprint.Width, f->Footprint.Height, f->Footprint.Format, f->Footprint.RowPitch);
        DumpBytes("data", s, f->Offset, (uint64_t)f->Footprint.RowPitch * f->Footprint.Height);
    }
    fprintf(trace, "\n");
    REAL(PfnCopyTextureRegion, 111)(This, dst, x, y, z, src, box);
}

typedef void (STDMETHODCALLTYPE* PfnClearUavFloat)(ID3D12GraphicsCommandList*, D3D12_GPU_DESCRIPTOR_HANDLE, D3D12_CPU_DESCRIPTOR_HANDLE, ID3D12Resource*, const FLOAT[4], UINT, const D3D12_RECT*);
static void STDMETHODCALLTYPE HookClearUavFloat(ID3D12GraphicsCommandList* This, D3D12_GPU_DESCRIPTOR_HANDLE g, D3D12_CPU_DESCRIPTOR_HANDLE c, ID3D12Resource* r, const FLOAT v[4], UINT n, const D3D12_RECT* rects) {
    ResInfo* ri = Res(r);
    fprintf(trace, "CLEARUAVFLOAT r%d %g %g %g %g\n", ri ? ri->id : -1, v[0], v[1], v[2], v[3]);
    REAL(PfnClearUavFloat, 112)(This, g, c, r, v, n, rects);
}
typedef void (STDMETHODCALLTYPE* PfnClearUavUint)(ID3D12GraphicsCommandList*, D3D12_GPU_DESCRIPTOR_HANDLE, D3D12_CPU_DESCRIPTOR_HANDLE, ID3D12Resource*, const UINT[4], UINT, const D3D12_RECT*);
static void STDMETHODCALLTYPE HookClearUavUint(ID3D12GraphicsCommandList* This, D3D12_GPU_DESCRIPTOR_HANDLE g, D3D12_CPU_DESCRIPTOR_HANDLE c, ID3D12Resource* r, const UINT v[4], UINT n, const D3D12_RECT* rects) {
    ResInfo* ri = Res(r);
    fprintf(trace, "CLEARUAVUINT r%d %u %u %u %u\n", ri ? ri->id : -1, v[0], v[1], v[2], v[3]);
    REAL(PfnClearUavUint, 113)(This, g, c, r, v, n, rects);
}

// Every buffer whose address the upscaler takes (constant buffer rings, scratch) becomes known.
static D3D12_GPU_VIRTUAL_ADDRESS STDMETHODCALLTYPE HookGetGpuVa(ID3D12Resource* This) {
    Res(This);
    return ((PfnGetGpuVa)real_get_va)(This);
}

void CaptureInstall(ID3D12Device* device, ID3D12GraphicsCommandList* list, const char* directory) {
    snprintf(dir, sizeof(dir), "%s", directory);
    CreateDirectoryA(dir, NULL);
    char path[MAX_PATH];
    snprintf(path, sizeof(path), "%s\\trace.txt", dir);
    trace = fopen(path, "w");
    setvbuf(trace, NULL, _IOFBF, 1 << 20);
    device_vtbl = *(void***)device;
    list_vtbl = *(void***)list;
    Patch(device_vtbl, SLOT(ID3D12DeviceVtbl, CreateComputePipelineState), HookCreateComputePipelineState, 0);
    Patch(device_vtbl, SLOT(ID3D12DeviceVtbl, CreateRootSignature), HookCreateRootSignature, 1);
    Patch(device_vtbl, SLOT(ID3D12DeviceVtbl, CreateDescriptorHeap), HookCreateDescriptorHeap, 2);
    Patch(device_vtbl, SLOT(ID3D12DeviceVtbl, CreateConstantBufferView), HookCreateCBV, 3);
    Patch(device_vtbl, SLOT(ID3D12DeviceVtbl, CreateShaderResourceView), HookCreateSRV, 4);
    Patch(device_vtbl, SLOT(ID3D12DeviceVtbl, CreateUnorderedAccessView), HookCreateUAV, 5);
    Patch(device_vtbl, SLOT(ID3D12DeviceVtbl, CreateSampler), HookCreateSampler, 6);
    Patch(device_vtbl, SLOT(ID3D12DeviceVtbl, CopyDescriptorsSimple), HookCopyDescriptorsSimple, 7);
    Patch(device_vtbl, SLOT(ID3D12DeviceVtbl, CopyDescriptors), HookCopyDescriptors, 8);
    // Resources: one vtable for committed and placed ones; take it from a small buffer.
    {
        D3D12_HEAP_PROPERTIES heap = {.Type = D3D12_HEAP_TYPE_UPLOAD};
        D3D12_RESOURCE_DESC desc = {.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER, .Width = 256, .Height = 1,
                                    .DepthOrArraySize = 1, .MipLevels = 1, .SampleDesc = {1, 0},
                                    .Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR};
        ID3D12Resource* probe = NULL;
        if (SUCCEEDED(ID3D12Device_CreateCommittedResource(device, &heap, D3D12_HEAP_FLAG_NONE, &desc,
                D3D12_RESOURCE_STATE_GENERIC_READ, NULL, &IID_ID3D12Resource, (void**)&probe))) {
            void** vtbl = *(void***)probe;
            Patch(vtbl, SLOT(ID3D12ResourceVtbl, GetGPUVirtualAddress), HookGetGpuVa, 300);
            real_get_va = real[300];
        }
    }
    Patch(list_vtbl, SLOT(ID3D12GraphicsCommandListVtbl, SetComputeRootSignature), HookSetComputeRootSignature, 100);
    Patch(list_vtbl, SLOT(ID3D12GraphicsCommandListVtbl, SetPipelineState), HookSetPipelineState, 101);
    Patch(list_vtbl, SLOT(ID3D12GraphicsCommandListVtbl, SetComputeRootDescriptorTable), HookSetComputeRootDescriptorTable, 102);
    Patch(list_vtbl, SLOT(ID3D12GraphicsCommandListVtbl, SetComputeRoot32BitConstant), HookSetComputeRoot32BitConstant, 103);
    Patch(list_vtbl, SLOT(ID3D12GraphicsCommandListVtbl, SetComputeRoot32BitConstants), HookSetComputeRoot32BitConstants, 104);
    Patch(list_vtbl, SLOT(ID3D12GraphicsCommandListVtbl, SetComputeRootConstantBufferView), HookSetComputeRootCBV, 105);
    Patch(list_vtbl, SLOT(ID3D12GraphicsCommandListVtbl, SetComputeRootShaderResourceView), HookSetComputeRootSRV, 106);
    Patch(list_vtbl, SLOT(ID3D12GraphicsCommandListVtbl, SetComputeRootUnorderedAccessView), HookSetComputeRootUAV, 107);
    Patch(list_vtbl, SLOT(ID3D12GraphicsCommandListVtbl, Dispatch), HookDispatch, 108);
    Patch(list_vtbl, SLOT(ID3D12GraphicsCommandListVtbl, CopyBufferRegion), HookCopyBufferRegion, 109);
    Patch(list_vtbl, SLOT(ID3D12GraphicsCommandListVtbl, CopyResource), HookCopyResource, 110);
    Patch(list_vtbl, SLOT(ID3D12GraphicsCommandListVtbl, CopyTextureRegion), HookCopyTextureRegion, 111);
    Patch(list_vtbl, SLOT(ID3D12GraphicsCommandListVtbl, ClearUnorderedAccessViewFloat), HookClearUavFloat, 112);
    Patch(list_vtbl, SLOT(ID3D12GraphicsCommandListVtbl, ClearUnorderedAccessViewUint), HookClearUavUint, 113);
}

void CaptureMark(const char* text) {
    if (trace) { fprintf(trace, "MARK %s\n", text); fflush(trace); }
}

void CaptureNoteResource(ID3D12Resource* r, const char* name) {
    ResInfo* info = Res(r);
    if (info) fprintf(trace, "NAME r%d %s\n", info->id, name);
}
