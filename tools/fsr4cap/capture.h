// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: D3D12 call recorder for fsr4cap.exe (capture.c, rootsig.c).
#pragma once
#include <stdio.h>
#include <d3d12.h>

/// Hooks the vtables of `device` and `list` (all objects of their classes); writes to `directory`.
void CaptureInstall(ID3D12Device* device, ID3D12GraphicsCommandList* list, const char* directory);
/// A line in the trace (frame boundaries).
void CaptureMark(const char* text);
/// Names a resource in the trace (the inputs and output of the test).
void CaptureNoteResource(ID3D12Resource* resource, const char* name);
/// Writes the parameters of a serialized root signature.
void CaptureDescribeRootSignature(FILE* out, const void* blob, size_t size);
