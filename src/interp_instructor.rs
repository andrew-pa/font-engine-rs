use super::*;
use numerics::*;
use truetype_loader::*;

#[derive(Debug)]
pub enum ScalerError {
    MissingTable(TableTag),
    InvalidInstruction(usize, u8),
    StackUnderflow(usize),
    InvalidGlyph
}

impl Error for ScalerError {
    fn description(&self) -> &str {
        match self {
            &ScalerError::MissingTable(_) => "missing font data table",
            &ScalerError::InvalidInstruction(_,_) => "invalid instruction encountered",
            &ScalerError::StackUnderflow(_) => "stack underflow",
            &ScalerError::InvalidGlyph => "glyph data invalid"
        }
    }
}

impl std::fmt::Display for ScalerError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            &ScalerError::MissingTable(t) => write!(f, "missing font data table {:?}", t),
            &ScalerError::InvalidInstruction(pc, istr) => write!(f, "invalid instruction at {:x}, code: {:2x}", pc, istr),
            &ScalerError::StackUnderflow(pc) => write!(f, "stack underflow at {:x}", pc),
            &ScalerError::InvalidGlyph => write!(f, "glyph data invalid"),
            _ => write!(f, "{}", self.description())
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct Vector {
    x: f32, y: f32
}

impl Vector {
    fn len(&self) -> f32 {
        (self.x*self.x + self.y*self.y).sqrt()
    }

    fn project(&self, p: Point) -> f32 {
        let len = self.len();
        p.x * self.x / len + p.y * self.y / len
    }
}

#[derive(Debug, Clone)]
struct InterpState {
    auto_flip: bool,
    cvt_cutin: f32,
    delta_base: u32,
    delta_shift: u32,
    dual_prj_vec: Vector,
    freedom_vec: Vector,
    instruct_ctrl: bool,
    loopv: u32,
    min_dist: f32,
    project_vec: Vector,
    round_state: u32,
    rp: [usize; 3],
    scan_ctrl: bool,
    single_width_cut_in: f32,
    single_width_value: f32,
    zp: [usize; 3],
    twilight_zone: Vec<Point>,
    cv_table: Vec<i16>,
}

impl InterpState {
    fn new(cv_table: Vec<i16>) -> InterpState {
        InterpState {
            auto_flip: true,
            cvt_cutin: 17.0 / 16.0,
            delta_base: 9,
            delta_shift: 3,
            dual_prj_vec: Vector { x: 0.0, y: 0.0 },
            freedom_vec: Vector { x: 1.0, y: 0.0 },
            instruct_ctrl: false,
            loopv: 1,
            min_dist: 1.0,
            project_vec: Vector { x: 1.0, y: 0.0 },
            round_state: 1,
            rp: [0,0,0],
            scan_ctrl: false,
            single_width_cut_in: 0.0,
            single_width_value: 0.0,
            zp: [1,1,1],
            twilight_zone: Vec::new(),
            cv_table
        }
    }
}


fn sign_extend(v: u16) -> u32 { 
    0
}

struct Interp<'s, 'p> {
    stack: Vec<u32>,
    pc: usize,
    state: &'s mut InterpState,
    original_points: Vec<Point>,
    points: &'p mut Vec<Point>,
    uniform_scale: f32,
    units_per_em: f32,
    point_size: f32,
}

impl<'s, 'p> Interp<'s, 'p> {
    fn new<'f>(scaler: &'s mut InstructedGlyphScaler<'f>, points: &'p mut Vec<Point>) -> Interp<'s, 'p> {
        let op = points.clone();
        let uniform_scale = scaler.uniform_scale();
        Interp {
            stack: Vec::new(),
            pc: 0,
            state: &mut scaler.state, points,
            original_points: op,
            uniform_scale,
            units_per_em: scaler.units_per_em,
            point_size: scaler.point_size
        }
    }

    fn reset(&mut self) {
        self.stack.clear();
        self.pc = 0;
    }

    fn pop(&mut self) -> Result<u32, ScalerError> {
        self.stack.pop().ok_or(ScalerError::StackUnderflow(self.pc))
    }
    fn pop_f26dot6(&mut self) -> Result<F26d6, ScalerError> {
        self.stack.pop().map(|x| F26d6::from(x as i32)).ok_or(ScalerError::StackUnderflow(self.pc))
    }

    fn push(&mut self, v: u32) {
        self.stack.push(v);
    }

    fn compare<F: FnOnce(u32,u32)->bool>(&mut self, f: F) -> Result<(), ScalerError> {
        let (a,b) = (self.pop()?, self.pop()?);
        self.push(if f(a,b) { 1 } else { 0 });
        Ok(())
    }

    fn push_bytes(&mut self, n: usize, instructions: &Vec<u8>) -> Result<(), ScalerError> {
        println!("reading {} bytes", n);
        for i in self.pc+1..self.pc+n+1 {
            self.push(instructions[i] as u32);
        }
        self.pc += n;
        Ok(())
    }
    fn push_words(&mut self, n: usize, instructions: &Vec<u8>) -> Result<(), ScalerError> {
        println!("reading {} words", n);
        for i in self.pc+1..self.pc+n*2+1 {
            self.push(sign_extend((instructions[i] as u16) << 8 | instructions[i+1] as u16));
        }
        self.pc += n*2;
        Ok(())
    }


    fn interpret(&mut self, instructions: &Vec<u8>) -> Result<(), ScalerError> {
        while self.pc < instructions.len() {
            let l = self.stack.len();
            print!("pc = {:x}, current instruction = {:2x}, stack = [ ", self.pc, instructions[self.pc]);
            for i in 1..11 {
                if l >= i { print!("{:x} ", self.stack[l-i]); }
            }
            println!("]");
            match instructions[self.pc] {
                0x7f => {self.pop()?;},
                0x64 => { let v = self.pop_f26dot6()?.abs().into(); self.push(v) },
                0x60 => { let v = (self.pop_f26dot6()? + self.pop_f26dot6()?).into(); self.push(v) },
                0x27 => { /* ALIGN */ },
                0x3c => { /* ALIGNRP */ },
                0x5a => {
                    let (a, b) = (self.pop()?, self.pop()?);
                    self.push(if (a == 1) && (b == 1) { 1 } else { 0 })
                },
                0x2b => { /* CALL */ },
                0x67 => { let v = self.pop_f26dot6()?.ceil().into(); self.push(v) },
                0x25 => { /* CINDEX */ },
                0x22 => self.stack.clear(),
                0x4f => println!("debug value: {:x}", self.pop()?),
                0x73 => { /* DELTAC1 */ },
                0x74 => { /* DELTAC2 */ },
                0x75 => { /* DELTAC3 */ },
                0x5d => { /* DELTAP1 */ },
                0x71 => { /* DELTAP2 */ },
                0x72 => { /* DELTAP3 */ },
                0x24 => { let l = self.stack.len() as u32; self.push(l) },
                0x62 => { let v = (self.pop_f26dot6()? / self.pop_f26dot6()?).into(); self.push(v) },
                0x20 => { let t = self.stack[self.stack.len()-1]; self.push(t) }
                0x59 => { /* EIF */ /* nop */ },
                0x1b => { /* ELSE */ 
                    // only way to execute this instruction is if the true side of an IF branch
                    // was exectuted, so skip past EIF
                    while instructions[self.pc] != 0x59 {
                        self.pc += 1;
                    }
                    self.pc += 1;
                },
                0x2d => { /* ENDF */ },
                0x54 => self.compare(|a,b| a == b)?,
                0x57 => { /* EVEN */ },
                0x2c => { /* FDEF */ },
                0x4e => { self.state.auto_flip = false; },
                0x4d => { self.state.auto_flip = true; },
                0x80 => { /* FLIPPT */ },
                0x82 => { /* FLIPRGOFF */ },
                0x81 => { /* FLIPRGON */ },
                0x66 => { let v = self.pop_f26dot6()?.floor().into(); self.push(v) },
                0x46 => { /* GC[0] */ },
                0x47 => { /* GC[1] */ },
                0x88 => { println!("info req: {:b}", self.pop()?); self.push(0) },
                0x0d => { let (x,y) = (F26d6::from(self.state.freedom_vec.x), F26d6::from(self.state.freedom_vec.y)); self.push(x.into()); self.push(y.into()) },
                0x0c => { let (x,y) = (F26d6::from(self.state.project_vec.x), F26d6::from(self.state.project_vec.y)); self.push(x.into()); self.push(y.into()) },
                0x52 => self.compare(|a,b| a > b)?,
                0x53 => self.compare(|a,b| a >= b)?,
                0x89 => { /* IDEF */ },
                0x58 => { /* IF */
                    let cond = self.pop()?;
                    if cond == 0 {
                        // move to next ELSE or EIF instruction
                        while instructions[self.pc] != 0x1b || instructions[self.pc] != 0x59 {
                            self.pc += 1;
                        }
                        self.pc += 1; //move one past so ELSE doesn't jump to EIF
                    }
                },
                0x8e => { /* INSTCTRL [cvt only] */ panic!("INSTCTRL only in CVT programs"); },
                0x39 => { /* IP */ },
                0x0f => { /* ISECT */ },
                0x30 => { /* IUP[0] */ },
                0x31 => { /* IUP[1] */ },
                0x1c => { self.pc += (self.pop()? - 1) as usize; }
                0x79 => { let (e, offset) = (self.pop()?, self.pop()?); if e == 0 { self.pc += (offset-1) as usize; } }
                0x78 => { let (e, offset) = (self.pop()?, self.pop()?); if e == 1 { self.pc += (offset-1) as usize; } }
                0x2a => { /* LOOPCALL */ },
                0x50 => self.compare(|a,b| a < b)?,
                0x51 => self.compare(|a,b| a <= b)?,
                0x8b => { let v = self.pop()?.max(self.pop()?); self.push(v); },
                0x49 => { /* MD[0] */
                    let (p1, p2) = (self.pop()? as usize, self.pop()? as usize);
                    let (d1, d2) = (self.state.project_vec.project(self.points[p1]), self.state.project_vec.project(self.points[p2])); 
                    self.push(F26d6::from(d2-d1).into())
                },
                0x4a => { /* MD[1] */
                    let (p1, p2) = (self.pop()? as usize, self.pop()? as usize);
                    let (d1, d2) = (self.state.project_vec.project(self.original_points[p1]), self.state.project_vec.project(self.original_points[p2])); 
                    self.push(F26d6::from(d2-d1).into())
                },
                0x2e => { /* MDAP[0] */ },
                0x2f => { /* MDAP[1] */ },
                0xc0 ... 0xdf => { /* MDRP[abcde] */ },
                0x3e => { /* MIAP[0] */ },
                0x3f => { /* MIAP[1] */ },
                0x8c => { let v = self.pop()?.min(self.pop()?); self.push(v); },
                0x26 => { /* MINDEX */ },
                0xe0 ... 0xff => { /* MIRP[abcde] */ },
                0x4b => { let s = self.uniform_scale as u32; self.push(s) },
                0x4c => { let s = self.point_size as u32; self.push(s) },
                0x3a ... 0x3b => { /* MSIRP[a] */ },
                0x63 => { let v = (self.pop_f26dot6()? * self.pop_f26dot6()?).into(); self.push(v) },
                0x65 => { let v = (-self.pop_f26dot6()?).into(); self.push(v) },
                0x55 => self.compare(|a,b| a != b)?,
                0x5c => { let v = if self.pop()? == 0 { 1 } else { 0 }; self.push(v) },
                0x40 => { self.pc += 1; let len = instructions[self.pc] as usize; self.push_bytes(len, &instructions)? },
                0x41 => { self.pc += 1; let len = instructions[self.pc] as usize; self.push_words(len, &instructions)? },
                0x6c ... 0x6f => { /* NROUND[a] */ },
                0x56 => { /* ODD */ },
                0x5b => {
                    let (a, b) = (self.pop()?, self.pop()?);
                    self.push(if (a == 1) || (b == 1) { 1 } else { 0 })
                },
                0x21 => { self.pop()?; }
                0xb0 ... 0xb7 => { let len = instructions[self.pc] as usize - 0xaf; self.push_bytes(len,  &instructions)? },
                0xb8 ... 0xbf => { let len = instructions[self.pc] as usize - 0xb7; self.push_words(len, &instructions)? },
                0x45 => { /* RCVT */ },
                0x7d => { /* RDTG */ },
                0x7a => { /* ROFF */ },
                0x8a => {
                    let l = self.stack.len();
                    let a = self.stack[l-1];
                    self.stack[l-1] = self.stack[l-3];
                    self.stack[l-3] = a;
                },
                0x68 ... 0x6b => { /* ROUND[ab] */ },
                0x43 => { /* RS */ },
                0x3d => { /* RTDG */ },
                0x18 => { /* RTG */ },
                0x19 => { /* RTHG */ },
                0x7c => { /* RUTG */ },
                0x77 => { /* S45ROUND */ },
                0x7e => { self.pop()?; },
                0x85 => { /* SCANCTRL */ },
                0x8d => { /* SCANTYPE */ },
                0x48 => { /* SCFS */ },
                0x1d => { /* SCVTCI */ },
                0x5e => { self.state.delta_base = self.pop()?; },
                0x86 ... 0x87 => { /* SDPVTL */ },
                0x5f => { self.state.delta_shift = self.pop()?; },
                0x0b => { self.state.freedom_vec = Vector { x: self.pop_f26dot6()?.into(), y: self.pop_f26dot6()?.into() }; },
                0x04 => { self.state.freedom_vec = Vector { x: 0.0, y: 1.0 }; },
                0x05 => { self.state.freedom_vec = Vector { x: 1.0, y: 0.0 }; },
                0x08 => { /* SFVTL[0] */ },
                0x09 => { /* SFVTL[1] */ },
                0x0e => { self.state.freedom_vec = self.state.project_vec; },
                0x34 ... 0x35 => { /* SHC[a] */ },
                0x32 ... 0x33 => { /* SHP[a] */ },
                0x38 => { /* SHPIX */ },
                0x36 ... 0x37 => { /* SHZ */ },
                0x17 => { self.state.loopv = self.pop()?; },
                0x1a => { self.state.min_dist = self.pop_f26dot6()?.into(); },
                0x0a => { self.state.project_vec = Vector { x: self.pop_f26dot6()?.into(), y: self.pop_f26dot6()?.into() }; },
                0x02 => { self.state.project_vec = Vector { x: 0.0, y: 1.0 }; },
                0x03 => { self.state.project_vec = Vector { x: 1.0, y: 0.0 }; },
                0x06 ... 0x07 => { /* SPVTL */ },
                0x76 => { /* SROUND */ },
                0x10 => { self.state.rp[0] = self.pop()? as usize; },
                0x11 => { self.state.rp[1] = self.pop()? as usize; },
                0x12 => { self.state.rp[2] = self.pop()? as usize; },
                0x1f => { self.state.single_width_value = self.pop()? as f32; },
                0x1e => { self.state.single_width_cut_in = self.pop()? as f32; },
                0x61 => { let v = (self.pop_f26dot6()? - self.pop_f26dot6()?).into(); self.push(v) },
                0x00 => { self.state.freedom_vec = Vector { x: 0.0, y: 1.0 }; self.state.project_vec = Vector { x: 0.0, y: 1.0 }; },
                0x01 => { self.state.freedom_vec = Vector { x: 0.0, y: 1.0 }; self.state.project_vec = Vector { x: 1.0, y: 0.0 }; },
                0x23 => {
                    let l = self.stack.len();
                    let a = self.stack[l-1];
                    self.stack[l-1] = self.stack[l-2];
                    self.stack[l-2] = a;
                },
                0x13 => { self.state.zp[0] = self.pop()? as usize; },
                0x14 => { self.state.zp[1] = self.pop()? as usize; },
                0x15 => { self.state.zp[2] = self.pop()? as usize; },
                0x16 => { let p = self.pop()? as usize; self.state.zp[0] = p; self.state.zp[1] = p; self.state.zp[2] = p; },
                0x29 => { /* UTP */ },
                0x70 => { /* WCVTF */ },
                0x44 => { /* WCVTP */ },
                0x42 => { /* WS */ },

                _ => return Err(ScalerError::InvalidInstruction(self.pc, instructions[self.pc]))
            }
            self.pc += 1;
        }
        Ok(())
    }
}



pub struct InstructedGlyphScaler<'f> {
    glyph_table: &'f GlyphDataTable,
    output_dpi: f32,
    units_per_em: f32,
    point_size: f32,
    state: InterpState
}

impl<'f> InstructedGlyphScaler<'f> {


    pub fn new(font: &'f SfntFont, dpi: f32, point_size: f32) -> Result<InstructedGlyphScaler<'f>, ScalerError> {
        let mut slf = InstructedGlyphScaler {
            glyph_table: font.glyf_table.as_ref().ok_or(ScalerError::MissingTable(TableTag::GlyphData))?,
            output_dpi: dpi, point_size,
            units_per_em: font.head_table.ok_or(ScalerError::MissingTable(TableTag::FontHeader))?.units_per_em as f32,
            state: InterpState::new(font.cval_table.as_ref().ok_or(ScalerError::MissingTable(TableTag::ControlValue))?.0.clone())
        };
        println!("font program");
        if let Some(ref fprg) = font.fprg_table {
            Interp::new(&mut slf, &mut Vec::new()).interpret(&fprg.0)?;
        }
        println!("preprogram");
        if let Some(ref prep) = font.prep_table {
            Interp::new(&mut slf, &mut Vec::new()).interpret(&prep.0)?;
        }
        Ok(slf)
    }
}

impl<'f> GlyphScaler for InstructedGlyphScaler<'f> {
    fn uniform_scale(&self) -> f32 {
        self.point_size * self.output_dpi / (72f32 * self.units_per_em)
    }
    fn scale_glyph(&mut self, glyph_index: usize, offset: Point) -> Result<Glyph, Box<Error>> {
        let scale = self.uniform_scale();
        if let GlyphDescription::Simple { num_contours, ref end_points_of_contours, ref instructions, ref points, .. } = self.glyph_table.glyphs[glyph_index] {
            let mut points: Vec<Point> = points.iter().map(|&p| Point { x: p.x as f32*scale, y: p.y as f32*scale }).collect();
            
            Interp::new(self, &mut points).interpret(instructions)?;

            for p in points.iter_mut() {
                p.x += offset.x; 
                p.y += offset.y;
            }
            Glyph::from_truetype_with_points(&self.glyph_table.glyphs[glyph_index], points).ok_or(Box::new(ScalerError::InvalidGlyph))
        } else {
            Err(Box::new(ScalerError::InvalidGlyph))
        }
    }
}
