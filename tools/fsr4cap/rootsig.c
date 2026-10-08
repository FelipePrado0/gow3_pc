// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: prints a serialized D3D12 root signature (fsr4cap capture).
#define COBJMACROS
#define WIDL_C_INLINE_WRAPPERS
#include <windows.h>
#include <d3d12.h>
#include <stdio.h>

#include "capture.h"

static const char* RangeType(D3D12_DESCRIPTOR_RANGE_TYPE t) {
    switch (t) {
    case D3D12_DESCRIPTOR_RANGE_TYPE_SRV: return "SRV";
    case D3D12_DESCRIPTOR_RANGE_TYPE_UAV: return "UAV";
    case D3D12_DESCRIPTOR_RANGE_TYPE_CBV: return "CBV";
    default: return "SAMPLER";
    }
}

void CaptureDescribeRootSignature(FILE* out, const void* blob, size_t size) {
    ID3D12VersionedRootSignatureDeserializer* d = NULL;
    if (FAILED(D3D12CreateVersionedRootSignatureDeserializer(blob, size, &IID_ID3D12VersionedRootSignatureDeserializer, (void**)&d))) {
        fprintf(out, "  (cannot deserialize)\n");
        return;
    }
    const D3D12_VERSIONED_ROOT_SIGNATURE_DESC* v = NULL;
    if (FAILED(ID3D12VersionedRootSignatureDeserializer_GetRootSignatureDescAtVersion(d, D3D_ROOT_SIGNATURE_VERSION_1_1, &v)) || !v) {
        fprintf(out, "  (no 1.1 desc)\n");
        ID3D12VersionedRootSignatureDeserializer_Release(d);
        return;
    }
    const D3D12_ROOT_SIGNATURE_DESC1* rs = &v->Desc_1_1;
    for (UINT i = 0; i < rs->NumParameters; ++i) {
        const D3D12_ROOT_PARAMETER1* p = &rs->pParameters[i];
        fprintf(out, "  param %u: ", i);
        switch (p->ParameterType) {
        case D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE:
            fprintf(out, "table");
            for (UINT k = 0; k < p->DescriptorTable.NumDescriptorRanges; ++k) {
                const D3D12_DESCRIPTOR_RANGE1* r = &p->DescriptorTable.pDescriptorRanges[k];
                fprintf(out, " [%s x%u reg %u space %u offset %d]", RangeType(r->RangeType), r->NumDescriptors,
                        r->BaseShaderRegister, r->RegisterSpace, (int)r->OffsetInDescriptorsFromTableStart);
            }
            break;
        case D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS:
            fprintf(out, "constants x%u reg b%u space %u", p->Constants.Num32BitValues, p->Constants.ShaderRegister, p->Constants.RegisterSpace);
            break;
        default:
            fprintf(out, "%s reg %u space %u", p->ParameterType == D3D12_ROOT_PARAMETER_TYPE_CBV ? "CBV" :
                    p->ParameterType == D3D12_ROOT_PARAMETER_TYPE_SRV ? "SRV" : "UAV",
                    p->Descriptor.ShaderRegister, p->Descriptor.RegisterSpace);
        }
        fprintf(out, "\n");
    }
    for (UINT i = 0; i < rs->NumStaticSamplers; ++i) {
        const D3D12_STATIC_SAMPLER_DESC* s = &rs->pStaticSamplers[i];
        fprintf(out, "  static sampler s%u space %u filter 0x%x address %d,%d,%d\n", s->ShaderRegister, s->RegisterSpace,
                s->Filter, s->AddressU, s->AddressV, s->AddressW);
    }
    ID3D12VersionedRootSignatureDeserializer_Release(d);
}
