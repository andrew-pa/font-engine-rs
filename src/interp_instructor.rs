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

#[derive(Debug)]
struct InterpState {
    stack: Vec<u32>,
    pc: usize,
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
    zp: [usize; 3]
}

impl Default for InterpState {
    fn default() -> InterpState {
        InterpState {
            stack: Vec::new(),
            pc: 0,
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
            zp: [1,1,1]
        }
    }
}

fn sign_extend(v: u16) -> u32 { 
    0
}

impl InterpState {
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
        for i in self.pc..self.pc+n {
            self.push(instructions[i] as u32);
        }
        self.pc += n;
        Ok(())
    }
    fn push_words(&mut self, n: usize, instructions: &Vec<u8>) -> Result<(), ScalerError> {
        println!("reading {} words", n);
        for i in self.pc..self.pc+n*2 {
            self.push(sign_extend((instructions[i] as u16) << 8 | instructions[i+1] as u16));
        }
        self.pc += n*2;
        Ok(())
    }
}



pub struct InstructedGlyphScaler<'f> {
    glyph_table: &'f GlyphDataTable,
    cv_table: &'f ControlValueTable,
    output_dpi: f32,
    units_per_em: f32
}

impl<'f> InstructedGlyphScaler<'f> {
    pub fn new(font: &'f SfntFont, dpi: f32) -> Result<InstructedGlyphScaler<'f>, ScalerError> {
        Ok(InstructedGlyphScaler {
            glyph_table: font.glyf_table.as_ref().ok_or(ScalerError::MissingTable(TableTag::GlyphData))?,
            cv_table: font.cval_table.as_ref().ok_or(ScalerError::MissingTable(TableTag::ControlValue))?,
            output_dpi: dpi,
            units_per_em: font.head_table.ok_or(ScalerError::MissingTable(TableTag::FontHeader))?.units_per_em as f32
        })
    }
}

impl<'f> GlyphScaler for InstructedGlyphScaler<'f> {
    fn uniform_scale(&self, point_size: f32) -> f32 {
        point_size * self.output_dpi / (72f32 * self.units_per_em)
    }
    fn scale_glyph(&self, point_size: f32, glyph_index: usize, offset: Point) -> Result<Glyph, Box<Error>> {
        let scale = self.uniform_scale(point_size);
        if let GlyphDescription::Simple { num_contours, ref end_points_of_contours, ref instructions, ref points, .. } = self.glyph_table.glyphs[glyph_index] {
            let mut points: Vec<Point> = points.iter().map(|&p| Point { x: p.x as f32*scale, y: p.y as f32*scale }).collect();
            let original_points = points.clone();

            let mut state = InterpState::default();
            while state.pc < instructions.len() {
                println!("state = {:?}, current instruction = {:2x}", state, instructions[state.pc]);
                match instructions[state.pc] {
                    0x7f => {state.pop()?;},
                    0x64 => { let v = state.pop_f26dot6()?.abs().into(); state.push(v) },
                    0x60 => { let v = (state.pop_f26dot6()? + state.pop_f26dot6()?).into(); state.push(v) },
                    0x27 => { /* ALIGN */ },
                    0x3c => { /* ALIGNRP */ },
                    0x5a => {
                        let (a, b) = (state.pop()?, state.pop()?);
                        state.push(if (a == 1) && (b == 1) { 1 } else { 0 })
                    },
                    0x2b => { /* CALL */ },
                    0x67 => { let v = state.pop_f26dot6()?.ceil().into(); state.push(v) },
                    0x25 => { /* CINDEX */ },
                    0x22 => state.stack.clear(),
                    0x4f => println!("debug value: {:x}", state.pop()?),
                    0x73 => { /* DELTAC1 */ },
                    0x74 => { /* DELTAC2 */ },
                    0x75 => { /* DELTAC3 */ },
                    0x5d => { /* DELTAP1 */ },
                    0x71 => { /* DELTAP2 */ },
                    0x72 => { /* DELTAP3 */ },
                    0x24 => { let l = state.stack.len() as u32; state.push(l) },
                    0x62 => { let v = (state.pop_f26dot6()? / state.pop_f26dot6()?).into(); state.push(v) },
                    0x20 => { let t = state.stack[state.stack.len()-1]; state.push(t) }
                    0x59 => { /* EIF */ },
                    0x1b => { /* ELSE */  },
                    0x2d => { /* ENDF */ },
                    0x54 => state.compare(|a,b| a == b)?,
                    0x57 => { /* EVEN */ },
                    0x2c => { /* FDEF */ },
                    0x4e => { state.auto_flip = false; },
                    0x4d => { state.auto_flip = true; },
                    0x80 => { /* FLIPPT */ },
                    0x82 => { /* FLIPRGOFF */ },
                    0x81 => { /* FLIPRGON */ },
                    0x66 => { let v = state.pop_f26dot6()?.floor().into(); state.push(v) },
                    0x46 => { /* GC[0] */ },
                    0x47 => { /* GC[1] */ },
                    0x88 => { println!("info req: {:b}", state.pop()?); state.push(0) },
                    0x0d => { let (x,y) = (F26d6::from(state.freedom_vec.x), F26d6::from(state.freedom_vec.y)); state.push(x.into()); state.push(y.into()) },
                    0x0c => { let (x,y) = (F26d6::from(state.project_vec.x), F26d6::from(state.project_vec.y)); state.push(x.into()); state.push(y.into()) },
                    0x52 => state.compare(|a,b| a > b)?,
                    0x53 => state.compare(|a,b| a >= b)?,
                    0x89 => { /* IDEF */ },
                    0x58 => { /* IF */ },
                    0x8e => { /* INSTCTRL */ },
                    0x39 => { /* IP */ },
                    0x0f => { /* ISECT */ },
                    0x30 => { /* IUP[0] */ },
                    0x31 => { /* IUP[1] */ },
                    0x1c => { state.pc += (state.pop()? - 1) as usize; }
                    0x79 => { let (e, offset) = (state.pop()?, state.pop()?); if e == 0 { state.pc += (offset-1) as usize; } }
                    0x78 => { let (e, offset) = (state.pop()?, state.pop()?); if e == 1 { state.pc += (offset-1) as usize; } }
                    0x2a => { /* LOOPCALL */ },
                    0x50 => state.compare(|a,b| a < b)?,
                    0x51 => state.compare(|a,b| a <= b)?,
                    0x8b => { let v = state.pop()?.max(state.pop()?); state.push(v); },
                    0x49 => { /* MD[0] */
                        let (p1, p2) = (state.pop()? as usize, state.pop()? as usize);
                        let (d1, d2) = (state.project_vec.project(points[p1]), state.project_vec.project(points[p2])); 
                        state.push(F26d6::from(d2-d1).into())
                    },
                    0x4a => { /* MD[1] */
                        let (p1, p2) = (state.pop()? as usize, state.pop()? as usize);
                        let (d1, d2) = (state.project_vec.project(original_points[p1]), state.project_vec.project(original_points[p2])); 
                        state.push(F26d6::from(d2-d1).into())
                    },
                    0x2e => { /* MDAP[0] */ },
                    0x2f => { /* MDAP[1] */ },
                    0xc0 ... 0xdf => { /* MDRP[abcde] */ },
                    0x3e => { /* MIAP[0] */ },
                    0x3f => { /* MIAP[1] */ },
                    0x8c => { let v = state.pop()?.min(state.pop()?); state.push(v); },
                    0x26 => { /* MINDEX */ },
                    0xe0 ... 0xff => { /* MIRP[abcde] */ },
                    0x4b => state.push(scale as u32),
                    0x4c => state.push(point_size as u32),
                    0x3a ... 0x3b => { /* MSIRP[a] */ },
                    0x63 => { let v = (state.pop_f26dot6()? * state.pop_f26dot6()?).into(); state.push(v) },
                    0x65 => { let v = (-state.pop_f26dot6()?).into(); state.push(v) },
                    0x55 => state.compare(|a,b| a != b)?,
                    0x5c => { let v = if state.pop()? == 0 { 1 } else { 0 }; state.push(v) },
                    0x40 => { state.pc += 1; let len = instructions[state.pc] as usize; state.push_bytes(len, &instructions)? },
                    0x41 => { state.pc += 1; let len = instructions[state.pc] as usize; state.push_words(len, &instructions)? },
                    0x6c ... 0x6f => { /* NROUND[a] */ },
                    0x56 => { /* ODD */ },
                    0x5b => {
                        let (a, b) = (state.pop()?, state.pop()?);
                        state.push(if (a == 1) || (b == 1) { 1 } else { 0 })
                    },
                    0x21 => { state.pop()?; }
                    0xb0 ... 0xb7 => { let len = instructions[state.pc] as usize - 0xaf; state.push_bytes(len,  &instructions)? },
                    0xb8 ... 0xbf => { let len = instructions[state.pc] as usize - 0xb7; state.push_words(len, &instructions)? },
                    0x45 => { /* RCVT */ },
                    0x7d => { /* RDTG */ },
                    0x7a => { /* ROFF */ },
                    0x8a => {
                        let l = state.stack.len();
                        let a = state.stack[l-1];
                        state.stack[l-1] = state.stack[l-3];
                        state.stack[l-3] = a;
                    },
                    0x68 ... 0x6b => { /* ROUND[ab] */ },
                    0x43 => { /* RS */ },
                    0x3d => { /* RTDG */ },
                    0x18 => { /* RTG */ },
                    0x19 => { /* RTHG */ },
                    0x7c => { /* RUTG */ },
                    0x77 => { /* S45ROUND */ },
                    0x7e => { state.pop()?; },
                    0x85 => { /* SCANCTRL */ },
                    0x8d => { /* SCANTYPE */ },
                    0x48 => { /* SCFS */ },
                    0x1d => { /* SCVTCI */ },
                    0x5e => { state.delta_base = state.pop()?; },
                    0x86 ... 0x87 => { /* SDPVTL */ },
                    0x5f => { state.delta_shift = state.pop()?; },
                    0x0b => { state.freedom_vec = Vector { x: state.pop_f26dot6()?.into(), y: state.pop_f26dot6()?.into() }; },
                    0x04 => { state.freedom_vec = Vector { x: 0.0, y: 1.0 }; },
                    0x05 => { state.freedom_vec = Vector { x: 1.0, y: 0.0 }; },
                    0x08 => { /* SFVTL[0] */ },
                    0x09 => { /* SFVTL[1] */ },
                    0x0e => { state.freedom_vec = state.project_vec; },
                    0x34 ... 0x35 => { /* SHC[a] */ },
                    0x32 ... 0x33 => { /* SHP[a] */ },
                    0x38 => { /* SHPIX */ },
                    0x36 ... 0x37 => { /* SHZ */ },
                    0x17 => { state.loopv = state.pop()?; },
                    0x1a => { state.min_dist = state.pop_f26dot6()?.into(); },
                    0x0a => { state.project_vec = Vector { x: state.pop_f26dot6()?.into(), y: state.pop_f26dot6()?.into() }; },
                    0x02 => { state.project_vec = Vector { x: 0.0, y: 1.0 }; },
                    0x03 => { state.project_vec = Vector { x: 1.0, y: 0.0 }; },
                    0x06 ... 0x07 => { /* SPVTL */ },
                    0x76 => { /* SROUND */ },
                    0x10 => { state.rp[0] = state.pop()? as usize; },
                    0x11 => { state.rp[1] = state.pop()? as usize; },
                    0x12 => { state.rp[2] = state.pop()? as usize; },
                    0x1f => { state.single_width_value = state.pop()? as f32; },
                    0x1e => { state.single_width_cut_in = state.pop()? as f32; },
                    0x61 => { let v = (state.pop_f26dot6()? - state.pop_f26dot6()?).into(); state.push(v) },
                    0x00 => { state.freedom_vec = Vector { x: 0.0, y: 1.0 }; state.project_vec = Vector { x: 0.0, y: 1.0 }; },
                    0x01 => { state.freedom_vec = Vector { x: 0.0, y: 1.0 }; state.project_vec = Vector { x: 1.0, y: 0.0 }; },
                    0x23 => {
                        let l = state.stack.len();
                        let a = state.stack[l-1];
                        state.stack[l-1] = state.stack[l-2];
                        state.stack[l-2] = a;
                    },
                    0x13 => { state.zp[0] = state.pop()? as usize; },
                    0x14 => { state.zp[1] = state.pop()? as usize; },
                    0x15 => { state.zp[2] = state.pop()? as usize; },
                    0x16 => { let p = state.pop()? as usize; state.zp[0] = p; state.zp[1] = p; state.zp[2] = p; },
                    0x29 => { /* UTP */ },
                    0x70 => { /* WCVTF */ },
                    0x44 => { /* WCVTP */ },
                    0x42 => { /* WS */ },

                    _ => return Err(Box::new(ScalerError::InvalidInstruction(state.pc, instructions[state.pc])))
                }
                state.pc += 1;
            }

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
