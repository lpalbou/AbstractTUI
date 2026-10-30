"""Exercise the native ABI with ctypes; export frames for Wasm parity. No Qt required."""
import ctypes as C,sys
from pathlib import Path
lib=C.CDLL(sys.argv[1]);out=Path(sys.argv[2]);out.mkdir(parents=True,exist_ok=True)
lib.avatar_create.argtypes=[C.c_uint32,C.c_uint32];lib.avatar_create.restype=C.c_uint32
lib.avatar_set_shade.argtypes=[C.c_uint32]*4
lib.avatar_set_gaze.argtypes=[C.c_uint32,C.c_float,C.c_float]
lib.avatar_render.argtypes=[C.c_uint32]*3+[C.c_float,C.c_float,C.c_uint32]
lib.avatar_pixels.argtypes=[C.c_uint32];lib.avatar_pixels.restype=C.c_void_p
lib.avatar_pixels_len.argtypes=[C.c_uint32];lib.avatar_pixels_len.restype=C.c_size_t
lib.avatar_destroy.argtypes=[C.c_uint32]
h=lib.avatar_create(64,64);assert h
try:
 for m in range(3):
  for i in range(256):
   # JS Math.round for nonnegative values, rather than Python ties-to-even.
   rgba=(i<<24)|(int(i*.7+.5)<<16)|(int(i*.4+.5)<<8)|255
   assert lib.avatar_set_shade(h,m,i,rgba)==0
 for style in range(6):
  for pose in range(7):
   assert lib.avatar_render(h,style,pose,.6,1.1,1)==0
   (out/f'{style}-{pose}.rgba').write_bytes(C.string_at(lib.avatar_pixels(h),lib.avatar_pixels_len(h)))
 for style in range(6):
  for gaze in [-1,1]:
   assert lib.avatar_set_gaze(h,gaze,.6)==0
   assert lib.avatar_render(h,style,0,.6,1.1,1)==0
   (out/f'{style}-gaze-{gaze}.rgba').write_bytes(C.string_at(lib.avatar_pixels(h),lib.avatar_pixels_len(h)))
finally:assert lib.avatar_destroy(h)==0
print('Exported54 native ABI frames including gaze; handle released')
