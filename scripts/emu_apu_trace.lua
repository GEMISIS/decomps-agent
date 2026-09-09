-- Log CPU writes to APU registers ($4000-$4017, excluding $4014/$4016) per frame, with an optional button press.
local out = os.getenv("NESAPU_OUT"); local f0 = tonumber(os.getenv("NESAPU_FROM") or "0"); local f1 = tonumber(os.getenv("NESAPU_TO") or "300")
local press = os.getenv("NESAPU_PRESS"); local press_at = tonumber(os.getenv("NESAPU_PRESS_AT") or "0"); if press == "" then press = nil end
local f = io.open(out .. ".part", "w"); local frame = 0
local function hook(addr, size, value)
  if addr == 0x4014 or addr == 0x4016 then return end
  if frame >= f0 and frame <= f1 then f:write(string.format("f%d %04X<=%02X\n", frame, addr, value)) end
end
memory.registerwrite(0x4000, 0x18, hook)
while frame <= f1 do
  if press and ((frame >= press_at and frame <= press_at + 2) or (frame >= press_at + 40 and frame <= press_at + 42)) then
    local j = {A=false,B=false,select=false,start=false,up=false,down=false,left=false,right=false}; j[press] = true; joypad.set(1, j)
  end
  emu.frameadvance(); frame = frame + 1
end
f:close(); os.rename(out .. ".part", out)
-- no emu.exit(): it segfaults the Qt build (crash report per run); the shell wrapper stops the emulator with SIGTERM once the output file exists

