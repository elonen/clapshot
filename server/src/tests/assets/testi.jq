def kuppax($testi):
 "JOO \($testi)";

def frame_select_filter($thumbs; $total_frames):
  [
    range(0; $thumbs)
    | . as $pos
    | ($pos * ($total_frames / $thumbs) | floor)
    | "eq(n,\(.))"
  ]
  | join("+") + kuppax(666);


.THUMB_COUNT as $total_thumbs |
.total_frames as $total_frames |
frame_select_filter($total_thumbs; $total_frames)
