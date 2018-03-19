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

struct Vector {
    x: f32, y: f32
}

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
        for i in 0..n {
            self.push(instructions[self.pc + i] as u32);
        }
        self.pc += n;
        Ok(())
    }
    fn push_words(&mut self, n: usize, instructions: &Vec<u8>) -> Result<(), ScalerError> {
        for i in 0..n {
            state.push(sign_extend(instructions[state.pc + i] as u16 << 8 | instructions[state.pc + i + 1] as u16));
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

            let mut state = InterpState::default();
            while state.pc < instructions.len() {
                match instructions[state.pc] {
                    0x7f => {state.pop()?;},
                    0x64 => state.push(state.pop_f26dot6()?.abs().into()),
                    0x60 => state.push((state.pop_f26dot6()? + state.pop_f26dot6()?).into()),

                    0x22 => state.stack.clear(),
                    0x4f => println!("debug value: {:x}", state.pop()?),
                    0x24 => state.push(state.stack.len() as u32),
                    0x20 => state.push(state.stack[0]),
                    0x54 => state.compare(|a,b| a == b)?,
                    0x4e => { state.auto_flip = false; },
                    0x4d => { state.auto_flip = true; },
                    0x66 => { state.push(state.pop_f26dot6()?.floor().into()) },
                    0x88 => { println!("info req: {:b}", state.pop()?); state.push(0) },
                    0x0d => { state.push(F26d6::from(state.freedom_vec.x)); state.push(F26d6::from(state.freedom_vec.y)) },
                    0x0c => { state.push(F26d6::from(state.project_vec.x)); state.push(F26d6::from(state.project_vec.y)) },
                    0x52 => state.compare(|a,b| a > b)?,
                    0x53 => state.compare(|a,b| a >= b)?,
                    0x1c => { state.pc += state.pop()? - 1; }
                    0x79 => { let (e, offset) = (state.pop()?, state.pop()?); if e == 0 { state.pc += offset-1; } }
                    0x78 => { let (e, offset) = (state.pop()?, state.pop()?); if e == 1 { state.pc += offset-1; } }
                    0x50 => state.compare(|a,b| a < b)?,
                    0x51 => state.compare(|a,b| a <= b)?,
                    0x8b => state.push(state.pop()?.max(state.pop()?)),
                    0x8c => state.push(state.pop()?.min(state.pop()?)),
                    0x4b => state.push(scale as u32),
                    0x4c => state.push(point_size as u32),
                    0x63 => state.push((state.pop_f26dot6()? * state.pop_f26dot6()?).into()),
                    0x65 => state.push((-state.pop_f26dot6()?).into()),
                    0x55 => state.compare(|a,b| a != b)?,
                    0x5c => state.push(if state.pop()? == 0 { 1 } else { 0 }),
                    0x40 => { state.pc += 1; state.push_bytes(instructions[state.pc] as usize, &instructions)? },
                    0x41 => { state.pc += 1; state.push_words(instructions[state.pc] as usize, &instructions)? },
                    0x5b => {
                        let (a, b) = (state.pop()?, state.pop()?);
                        state.push(if (a == 1) || (b == 1) { 1 } else { 0 })
                    },
                    0x21 => { state.pop()?; }
                    0xb0 => state.push_bytes(1, &instructions)?,
                    0xb1 => state.push_bytes(2, &instructions)?,
                    0xb2 => state.push_bytes(3, &instructions)?,
                    0xb3 => state.push_bytes(4, &instructions)?,
                    0xb4 => state.push_bytes(5, &instructions)?,
                    0xb5 => state.push_bytes(6, &instructions)?,
                    0xb6 => state.push_bytes(7, &instructions)?,
                    0xb7 => state.push_bytes(8, &instructions)?,
                    0xb8 => state.push_words(1, &instructions)?,
                    0xb9 => state.push_words(2, &instructions)?,
                    0xba => state.push_words(3, &instructions)?,
                    0xbb => state.push_words(4, &instructions)?,
                    0xbc => state.push_words(5, &instructions)?,
                    0xbd => state.push_words(6, &instructions)?,
                    0xbe => state.push_words(7, &instructions)?,
                    0xbf => state.push_words(8, &instructions)?,

                    0x8a => {
                        let a = state.stack[0];
                        state.stack[0] = state.stack[2];
                        state.stack[2] = a;
                    },

                    0x7e => { state.pop()? },
                    
                    0x5e => { state.delta_base = state.pop()?; },

                    0x5f => { state.delta_shift = state.pop()?; },

                    0x0b => { state.freedom_vec = Vector { x: state.pop_f26dot6()?.into_f32(), y: state.pop_f26dot6()?.into_f32() }; },
                    0x04 => { state.freedom_vec = Vector { x: 0.0, y: 1.0 }; },
                    0x05 => { state.freedom_vec = Vector { x: 1.0, y: 0.0 }; },

                    0x0e => { state.freedom_vec = state.project_vec; },
                    0x17 => { state.loopv = state.pop()?; },
                    0x1a => { state.min_dist = state.pop_f26dot6()?.into_f32(); },
                    0x0a => { state.project_vec = Vector { x: state.pop_f26dot6()?.into_f32(), y: state.pop_f26dot6()?.into_f32() }; },
                    0x02 => { state.project_vec = Vector { x: 0.0, y: 1.0 }; },
                    0x03 => { state.project_vec = Vector { x: 1.0, y: 0.0 }; },
                    
                    0x10 => { state.rp[0] = state.pop()? as usize; },
                    0x11 => { state.rp[1] = state.pop()? as usize; },
                    0x12 => { state.rp[2] = state.pop()? as usize; },

                    0x1f => { state.single_width_value = state.pop()?; },
                    0x1e => { state.single_width_cut_in = state.pop()?; },
                    0x61 => state.push((state.pop_f26dot6()? - state.pop_f26dot6()?).into()),
                    0x00 => { state.freedom_vec = state.project_vec = Vector { x: 0.0, y: 1.0 }; },
                    0x01 => { state.freedom_vec = state.project_vec = Vector { x: 1.0, y: 0.0 }; },
                    
                    0x23 => {
                        let a = state.stack[0];
                        state.stack[0] = state.stack[1];
                        state.stack[1] = a;
                    },

                    0x13 => { state.zp[0] = state.pop()? as usize; },
                    0x14 => { state.zp[1] = state.pop()? as usize; },
                    0x15 => { state.zp[2] = state.pop()? as usize; },
                    0x16 => { state.zp[0] = state.zp[1] = state.zp[2] = state.pop()? as usize; },

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
