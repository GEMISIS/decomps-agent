-- Measure CPU cycles per frame spent in one routine: from execution of NESCYC_ADDR
-- (hex, e.g. the address of _audio_tick from the ld65 map) to the last APU register
-- write of that frame. Writes "frame cycles" lines for frames NESCYC_FROM..NESCYC_TO.
local out = os.getenv("NESCYC_OUT"); local f0 = tonumber(os.getenv("NESCYC_FROM") or "0"); local f1 = tonumber(os.getenv("NESCYC_TO") or "300")
local addr = tonumber(os.getenv("NESCYC_ADDR"), 16)
local press = os.getenv("NESCYC_PRESS"); local press_at = tonumber(os.getenv("NESCYC_PRESS_AT") or "0"); if press == "" then press = nil end
local f = io.open(out .. ".part", "w"); local frame = 0
local entry = nil; local last = nil
memory.registerexecute(addr, 1, function() entry = debugger.getcyclescount() end)
memory.registerwrite(0x4000, 0x10, function(a, size, value) if entry then last = debugger.getcyclescount() end end)
while frame <= f1 do
  if press and ((frame >= press_at and frame <= press_at + 2) or (frame >= press_at + 40 and frame <= press_at + 42)) then
    local j = {A=false,B=false,select=false,start=false,up=false,down=false,left=false,right=false}; j[press] = true; joypad.set(1, j)
  end
  entry = nil; last = nil
  emu.frameadvance()
  if frame >= f0 and entry and last then f:write(string.format("f%d %d\n", frame, last - entry)) end
  frame = frame + 1
end
f:close(); os.rename(out .. ".part", out)
-- no emu.exit(): it segfaults the Qt build (crash report per run); the shell wrapper stops the emulator with SIGTERM once the output file exists

