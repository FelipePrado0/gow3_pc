#!/usr/bin/env perl
# fsr4_post_lds.pl < post.comp > post_lds.comp
# gow3: rewrites a decompiled FSR 4 v07 post pass (spirv-cross GLSL) for RDNA3.
#
# Each invocation computes a 2x2 block of output pixels and stores them into three images
# (recurrent state, history, output). Stores of every other pixel from three images cost ~4x
# the arithmetic of the pass (2.9 of 6.1 ms at 2260x1272 -> 3840x2160 on an RX 7800 XT). Here
# the 8x8 workgroup collects its 16x16 block in shared memory and stores it in contiguous rows.
# Values stay float in shared memory and are converted by the store as before (a half value
# from shared memory would become a 16-bit store, which rounds differently), so the result is
# bit-exact with the original pass.
#
# spirv-cross mistranslates the signed int8 unpacks; Fsr4SpirvCrossFixes.pm corrects them.
use strict;
use warnings;
local $/;
my $src = <STDIN>;
die "unexpected local size\n"
    unless $src =~ /layout\(local_size_x = 8, local_size_y = 8, local_size_z = 1\) in;/;

use FindBin;
use lib $FindBin::Bin;
use Fsr4SpirvCrossFixes;
$src = Fsr4SpirvCrossFixes::signed_unpack($src);

my %slot = (rw_recurrent_0 => ['gow3_rec', 'vec4'], rw_history_color => ['gow3_hist', 'vec4'],
            rw_mlsr_output_color => ['gow3_out', 'vec4']);
for my $image (sort keys %slot) {
    my ($array, $type) = @{$slot{$image}};
    my $n = ($src =~ s/imageStore\($image, ivec2\((_\d+)\), /$array\[gow3Local($1)\] = $type(/g);
    die "expected one store to $image, found $n\n" unless $n == 1;
}
my $decl = <<'GLSL';
// gow3: the workgroup's 16x16 output block, stored in contiguous rows after the loop.
shared vec4 gow3_rec[256];
shared vec4 gow3_hist[256];
shared vec4 gow3_out[256];
uint gow3Local(uvec2 p)
{
    uvec2 l = p - gl_WorkGroupID.xy * 16u;
    return l.y * 16u + l.x;
}

void main()
GLSL
$src =~ s/^void main\(\)\n/$decl/m or die "no main\n";
my $flush = <<'GLSL';
    barrier();
    uvec2 gow3Base = gl_WorkGroupID.xy * 16u;
    for (uint gow3K = 0u; gow3K < 4u; gow3K++)
    {
        uint gow3I = gl_LocalInvocationIndex + 64u * gow3K;
        ivec2 gow3P = ivec2(gow3Base + uvec2(gow3I % 16u, gow3I / 16u));
        imageStore(rw_recurrent_0, gow3P, gow3_rec[gow3I]);
        imageStore(rw_history_color, gow3P, gow3_hist[gow3I]);
        imageStore(rw_mlsr_output_color, gow3P, gow3_out[gow3I]);
    }
}
GLSL
# main is the last function: its closing brace ends the file.
$src =~ s/\n}\s*\z/\n$flush/ or die "no end of main\n";
print $src;
