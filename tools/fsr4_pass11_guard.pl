#!/usr/bin/env perl
# fsr4_pass11_guard.pl < pass11.comp > pass11_guard.comp
# gow3: fixes a data race in FSR 4 v07 model pass 11 (decoder, 1/4 -> 1/2 resolution).
#
# Each invocation is a pixel of its 1/4-resolution input and writes a 2x2 block of the 1/2-
# resolution output. The dispatch rounds the width up to 64 invocations, and invocations past
# the input width are not stopped: at the 1080 tier (input 480 wide) invocations 480..511 write
# output pixels 960..1023, which are the first pixels of the next output row (every tensor row
# is 15360 bytes), racing with their real writers. The upscaled frame then differs from run to
# run in a strip at the left edge. Here they return first. The input size comes from the pass's
# own neighbour bounds check.
#
# spirv-cross mistranslates the signed int8 unpacks; Fsr4SpirvCrossFixes.pm corrects them.
use strict;
use warnings;
local $/;
my $src = <STDIN>;
my ($w, $h) = $src =~ /lessThan\(uvec3\(_\d+\), uvec3\((\d+)u, (\d+)u, 32u\)\)/
    or die "no input bounds check\n";
die "pass uses shared memory or barriers\n" if $src =~ /\bbarrier\(|\bshared\b/;
use FindBin;
use lib $FindBin::Bin;
use Fsr4SpirvCrossFixes;
$src = Fsr4SpirvCrossFixes::signed_unpack($src);
my $guard = <<"GLSL";
void main()
{
    // gow3: invocations past the input width would write the next row's first pixels.
    if (gl_GlobalInvocationID.x >= ${w}u || gl_GlobalInvocationID.y >= ${h}u)
    {
        return;
    }
GLSL
$src =~ s/^void main\(\)\n\{\n/$guard/m or die "no main\n";
print $src;
