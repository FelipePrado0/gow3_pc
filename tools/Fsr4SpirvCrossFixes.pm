# tools/Fsr4SpirvCrossFixes.pm: corrections to spirv-cross GLSL of the FSR 4 v07 passes,
# shared by the rewrite scripts (fsr4_post_lds.pl, fsr4_pass11_guard.pl).
package Fsr4SpirvCrossFixes;
use strict;
use warnings;

# spirv-cross translates OpBitcast uint -> v4char (signed int8) as unpack8(uint), which returns
# an unsigned u8vec4. The passes only ever unpack signed int8 tensors (no OpBitcast to v4uchar),
# so every unpack8 goes through a signed helper. Returns the source with the helper declared
# before main.
sub signed_unpack {
    my ($src) = @_;
    my $n = ($src =~ s/\bunpack8\(/gow3UnpackS8(/g);
    die "expected signed int8 unpacks\n" unless $n;
    my $helper = <<'GLSL';
// gow3: signed int8 unpack (spirv-cross emits the unsigned unpack8(uint)).
i8vec4 gow3UnpackS8(uint v)
{
    return unpack8(int(v));
}

GLSL
    $src =~ s/^(void main\(\)\n)/$helper$1/m or die "no main\n";
    return $src;
}

1;
