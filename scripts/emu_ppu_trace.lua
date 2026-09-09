-- Log CPU writes to PPU registers ($2000-$2007) with frame numbers, between two frames.
local out = os.getenv("NESTRACE_OUT") or "ppu_trace.txt"
local f0 = tonumber(os.getenv("NESTRACE_FROM") or "30"); local f1 = tonumber(os.getenv("NESTRACE_TO") or "36")
local press = os.getenv("NESTRACE_PRESS"); local press_at = tonumber(os.getenv("NESTRACE_PRESS_AT") or "0"); if press == "" then press = nil end
local f = io.open(out .. ".part", "w"); local frame = 0
local function hook(addr, size, value)
  if frame >= f0 and frame <= f1 then f:write(string.format("f%d %04X<=%02X\n", frame, addr, value)) end
end
for a = 0x2000, 0x2007 do memory.registerwrite(a, 1, hook) end
memory.registerexec(0, 0, nil)
while frame <= f1 do
  if press and ((frame >= press_at and frame <= press_at + 2) or (frame >= press_at + 40 and frame <= press_at + 42)) then
    local j = {A=false,B=false,select=false,start=false,up=false,down=false,left=false,right=false}; j[press] = true; joypad.set(1, j)
  end
  emu.frameadvance(); frame = frame + 1
end
f:close(); os.rename(out .. ".part", out); print("trace written")
-- no emu.exit(): it segfaults the Qt build (crash report per run); the shell wrapper stops the emulator with SIGTERM once the output file exists

