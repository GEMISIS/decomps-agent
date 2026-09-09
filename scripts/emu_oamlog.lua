-- Log the OAM records whose tile == NESOAM_TILE (or all visible when NESOAM_TILE < 0) for frames [NESOAM_FROM, NESOAM_TO]; press NESOAM_PRESS at NESOAM_AT (+2 frames).
local out = os.getenv("NESOAM_OUT"); local from = tonumber(os.getenv("NESOAM_FROM") or "1"); local to = tonumber(os.getenv("NESOAM_TO") or "60")
local press = os.getenv("NESOAM_PRESS"); local at = tonumber(os.getenv("NESOAM_AT") or "0"); local tile = tonumber(os.getenv("NESOAM_TILE") or "-1"); local hold = tonumber(os.getenv("NESOAM_HOLD") or "3")
if press == "" then press = nil end
local lines = {}
for i = 1, to do
  if press and i >= at and i < at + hold then local j = {A=false,B=false,select=false,start=false,up=false,down=false,left=false,right=false}; j[press] = true; joypad.set(1, j) end
  emu.frameadvance()
  if i >= from then
    local s = string.format("f%d", i)
    for k = 0, 63 do local y = memory.readbyte(0x0200+k*4); local t = memory.readbyte(0x0201+k*4); local a = memory.readbyte(0x0202+k*4); local x = memory.readbyte(0x0203+k*4)
      if (tile < 0 or t == tile) and y < 0xEF then s = s .. string.format("  [%02d] x=%3d y=%3d attr=%02X", k, x, y, a) end end
    lines[#lines+1] = s
  end
end
local f = io.open(out .. ".part", "w"); f:write(table.concat(lines, "\n") .. "\n"); f:close(); os.rename(out .. ".part", out)
-- no emu.exit(): it segfaults the Qt build (crash report per run); the shell wrapper stops the emulator with SIGTERM once the output file exists

