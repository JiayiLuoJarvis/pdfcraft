//! CFF1 subset for `FontFile3`. Keeps original glyph ids as `CharString` / CID indices.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{font_unusable, FontReason, Result};

const ENDCHAR: u8 = 14;
const CALLSUBR: u8 = 10;
const RETURN: u8 = 11;
const CALLGSUBR: u8 = 29;
const HINTMASK: u8 = 19;
const CNTRMASK: u8 = 20;
const HSTEM: u8 = 1;
const VSTEM: u8 = 3;
const HSTEMHM: u8 = 18;
const VSTEMHM: u8 = 23;
const ESCAPE: u8 = 12;

/// Subset a CFF1 font program to `used` glyph ids (plus `.notdef`).
///
/// Output is a CID-keyed CFF1 with Identity CIDs equal to the original glyph
/// ids. Subroutines are inlined so the subset carries no Subrs INDEX.
pub(crate) fn subset_cff(cff: &[u8], used: &BTreeMap<u16, (char, f32)>) -> Result<Vec<u8>> {
    let parsed = parse_cff(cff)?;
    let (kept, num_out, programs, fd_map) = build_subset_glyphs(&parsed, used)?;

    let mut string_buf = StringTable::from_existing(&parsed.strings);
    let adobe = string_buf.intern(b"Adobe");
    let identity = string_buf.intern(b"Identity");

    let mut out = Vec::new();
    out.extend_from_slice(&[1, 0, 4, 4]);
    write_index(&mut out, &[parsed.name.to_vec()])?;

    let (top, top_sites) = build_top_dict(&parsed, adobe, identity, num_out)?;
    write_index(&mut out, &[top])?;
    write_index(&mut out, &string_buf.bytes())?;
    write_index(&mut out, &[])?;

    let charset = out.len();
    write_identity_charset(&mut out, num_out)?;
    let private_meta = write_privates(&mut out, &parsed, &fd_map)?;
    let fd_select = out.len();
    write_fd_select(&mut out, num_out, &kept, &parsed, &fd_map)?;
    let fd_array = write_fd_array(&mut out, &parsed, &fd_map, &private_meta)?;
    let char_strings = out.len();
    write_index(&mut out, &programs)?;

    let top_values = TopDictValues {
        charset,
        char_strings,
        fd_array,
        fd_select,
    };
    patch_top_dict_offsets(&mut out, &top_sites, &top_values)?;
    Ok(out)
}

#[allow(clippy::struct_field_names)] // parallel placeholder sites in the top DICT
struct TopDictSites {
    charset: usize,
    char_strings: usize,
    fd_array: usize,
    fd_select: usize,
}

#[allow(clippy::struct_field_names)] // parallel absolute offsets patched into those sites
struct TopDictValues {
    charset: usize,
    char_strings: usize,
    fd_array: usize,
    fd_select: usize,
}

type GlyphBuild = (BTreeSet<u16>, usize, Vec<Vec<u8>>, BTreeMap<u8, u8>);
type FdLayout<'a> = (Vec<u8>, Vec<&'a [u8]>, Vec<Vec<&'a [u8]>>, Vec<Option<u16>>);

fn build_subset_glyphs(
    parsed: &ParsedCff<'_>,
    used: &BTreeMap<u16, (char, f32)>,
) -> Result<GlyphBuild> {
    let mut kept = BTreeSet::new();
    kept.insert(0);
    for &gid in used.keys() {
        if usize::from(gid) >= parsed.num_glyphs {
            return Err(head_error());
        }
        kept.insert(gid);
    }
    let num_out = usize::from(*kept.iter().next_back().unwrap()) + 1;
    let mut fd_used = BTreeSet::new();
    let mut charstrings = Vec::with_capacity(num_out);
    for gid in 0..num_out {
        let gid_u = u16::try_from(gid).map_err(|_| head_error())?;
        if !kept.contains(&gid_u) {
            charstrings.push(vec![ENDCHAR]);
            fd_used.insert(0_u8);
            continue;
        }
        let fd = parsed.fd_for(gid_u)?;
        fd_used.insert(fd);
        charstrings.push(desubroutinize(
            parsed.charstrings.get(gid).ok_or_else(head_error)?,
            &parsed.global_subrs,
            parsed.local_subrs_for(fd)?,
        )?);
    }
    let mut fd_map = BTreeMap::new();
    for (new, old) in fd_used.iter().copied().enumerate() {
        fd_map.insert(old, u8::try_from(new).map_err(|_| head_error())?);
    }
    if fd_map.is_empty() {
        fd_map.insert(0, 0);
    }
    Ok((kept, num_out, charstrings, fd_map))
}

fn build_top_dict(
    parsed: &ParsedCff<'_>,
    adobe: u16,
    identity: u16,
    num_out: usize,
) -> Result<(Vec<u8>, TopDictSites)> {
    let mut top = Vec::new();
    write_dict_sid(&mut top, parsed.version, 0);
    write_dict_sid(&mut top, parsed.notice, 1);
    write_dict_sid(&mut top, parsed.full_name, 2);
    write_dict_sid(&mut top, parsed.family_name, 3);
    write_dict_sid(&mut top, parsed.weight, 4);
    if let Some(bbox) = parsed.font_bbox {
        for v in bbox {
            write_dict_int(&mut top, v);
        }
        top.push(5);
    }
    if let Some(matrix) = &parsed.font_matrix {
        top.extend_from_slice(matrix);
    }
    write_dict_int(&mut top, i32::from(adobe));
    write_dict_int(&mut top, i32::from(identity));
    write_dict_int(&mut top, 0);
    top.extend_from_slice(&[ESCAPE, 30]);
    write_dict_int(&mut top, 0);
    top.extend_from_slice(&[ESCAPE, 31]);
    write_dict_int(&mut top, i32::try_from(num_out).map_err(|_| head_error())?);
    top.extend_from_slice(&[ESCAPE, 34]);

    let charset = top.len();
    write_dict_int5_placeholder(&mut top);
    top.push(15);
    let char_strings = top.len();
    write_dict_int5_placeholder(&mut top);
    top.push(17);
    let fd_array = top.len();
    write_dict_int5_placeholder(&mut top);
    top.extend_from_slice(&[ESCAPE, 36]);
    let fd_select = top.len();
    write_dict_int5_placeholder(&mut top);
    top.extend_from_slice(&[ESCAPE, 37]);
    Ok((
        top,
        TopDictSites {
            charset,
            char_strings,
            fd_array,
            fd_select,
        },
    ))
}

fn write_privates(
    out: &mut Vec<u8>,
    parsed: &ParsedCff<'_>,
    fd_map: &BTreeMap<u8, u8>,
) -> Result<Vec<(usize, usize)>> {
    let mut old_fds: Vec<u8> = fd_map.keys().copied().collect();
    old_fds.sort_unstable();
    let mut private_meta = Vec::new();
    for old_fd in old_fds {
        let private = strip_subrs_op(parsed.private_dict(old_fd)?)?;
        let offset = out.len();
        out.extend_from_slice(&private);
        private_meta.push((private.len(), offset));
    }
    Ok(private_meta)
}

fn write_fd_array(
    out: &mut Vec<u8>,
    parsed: &ParsedCff<'_>,
    fd_map: &BTreeMap<u8, u8>,
    private_meta: &[(usize, usize)],
) -> Result<usize> {
    let mut old_fds: Vec<u8> = fd_map.keys().copied().collect();
    old_fds.sort_unstable();
    let mut font_dicts = Vec::new();
    let mut private_patch_sites = Vec::new();
    for old_fd in old_fds.iter().copied() {
        let mut fd = Vec::new();
        if let Some(sid) = parsed.fd_font_name(old_fd) {
            write_dict_int(&mut fd, i32::from(sid));
            fd.extend_from_slice(&[ESCAPE, 38]);
        }
        let patch_at = fd.len();
        write_dict_int5_placeholder(&mut fd);
        write_dict_int5_placeholder(&mut fd);
        fd.push(18);
        private_patch_sites.push(patch_at);
        font_dicts.push(fd);
    }
    let fdarray_offset = out.len();
    let fdarray_index_start = out.len();
    write_index(out, &font_dicts)?;
    patch_font_dict_privates(
        out,
        fdarray_index_start,
        &font_dicts,
        &private_patch_sites,
        private_meta,
    )?;
    Ok(fdarray_offset)
}

struct ParsedCff<'a> {
    name: &'a [u8],
    strings: Vec<&'a [u8]>,
    global_subrs: Vec<&'a [u8]>,
    charstrings: Vec<&'a [u8]>,
    num_glyphs: usize,
    version: Option<u16>,
    notice: Option<u16>,
    full_name: Option<u16>,
    family_name: Option<u16>,
    weight: Option<u16>,
    font_bbox: Option<[i32; 4]>,
    font_matrix: Option<Vec<u8>>,
    /// Per-glyph FD index. Length equals `num_glyphs`. Name-keyed fonts use all `0`.
    fd_index: Vec<u8>,
    privates: Vec<&'a [u8]>,
    local_subrs: Vec<Vec<&'a [u8]>>,
    fd_font_names: Vec<Option<u16>>,
}

impl<'a> ParsedCff<'a> {
    fn fd_for(&self, gid: u16) -> Result<u8> {
        self.fd_index
            .get(usize::from(gid))
            .copied()
            .ok_or_else(head_error)
    }

    fn local_subrs_for(&self, fd: u8) -> Result<&[&'a [u8]]> {
        self.local_subrs
            .get(usize::from(fd))
            .map(Vec::as_slice)
            .ok_or_else(head_error)
    }

    fn private_dict(&self, fd: u8) -> Result<&'a [u8]> {
        self.privates
            .get(usize::from(fd))
            .copied()
            .ok_or_else(head_error)
    }

    fn fd_font_name(&self, fd: u8) -> Option<u16> {
        self.fd_font_names.get(usize::from(fd)).copied().flatten()
    }
}

fn parse_cff(data: &[u8]) -> Result<ParsedCff<'_>> {
    if data.len() < 4 || data[0] != 1 {
        return Err(head_error());
    }
    let hdr_size = usize::from(data[2]);
    if hdr_size > data.len() {
        return Err(head_error());
    }
    let mut i = hdr_size;
    let (next, names) = read_index(data, i)?;
    i = next;
    let name = names.first().copied().ok_or_else(head_error)?;
    let (next, top_dicts) = read_index(data, i)?;
    i = next;
    let top = top_dicts.first().copied().ok_or_else(head_error)?;
    let (next, strings) = read_index(data, i)?;
    i = next;
    let (_, global_subrs) = read_index(data, i)?;

    let top_fields = parse_dict_ops(top)?;
    let charstrings_off = *top_fields
        .get(&17)
        .and_then(|v| v.first())
        .ok_or_else(head_error)?;
    let charstrings_off = usize::try_from(charstrings_off).map_err(|_| head_error())?;
    let (_, charstrings) = read_index(data, charstrings_off)?;
    let num_glyphs = charstrings.len();
    if num_glyphs == 0 {
        return Err(head_error());
    }

    let (fd_index, privates, local_subrs, fd_font_names) = if top_fields.contains_key(&1230) {
        parse_cid_layout(data, &top_fields, num_glyphs)?
    } else {
        parse_name_keyed_layout(data, &top_fields, num_glyphs)?
    };

    Ok(ParsedCff {
        name,
        strings,
        global_subrs,
        charstrings,
        num_glyphs,
        version: sid_field(&top_fields, 0),
        notice: sid_field(&top_fields, 1),
        full_name: sid_field(&top_fields, 2),
        family_name: sid_field(&top_fields, 3),
        weight: sid_field(&top_fields, 4),
        font_bbox: top_fields.get(&5).and_then(|v| {
            if v.len() == 4 {
                Some([v[0], v[1], v[2], v[3]])
            } else {
                None
            }
        }),
        font_matrix: extract_operator_bytes(top, 1207)?,
        fd_index,
        privates,
        local_subrs,
        fd_font_names,
    })
}

fn parse_cid_layout<'a>(
    data: &'a [u8],
    top_fields: &BTreeMap<u16, Vec<i32>>,
    num_glyphs: usize,
) -> Result<FdLayout<'a>> {
    let fdarray_off = usize::try_from(
        *top_fields
            .get(&1236)
            .and_then(|v| v.first())
            .ok_or_else(head_error)?,
    )
    .map_err(|_| head_error())?;
    let fdselect_off = usize::try_from(
        *top_fields
            .get(&1237)
            .and_then(|v| v.first())
            .ok_or_else(head_error)?,
    )
    .map_err(|_| head_error())?;
    let (_, font_dicts) = read_index(data, fdarray_off)?;
    let mut privates = Vec::new();
    let mut local_subrs = Vec::new();
    let mut fd_font_names = Vec::new();
    for font_dict in &font_dicts {
        let fields = parse_dict_ops(font_dict)?;
        fd_font_names.push(
            fields
                .get(&1238)
                .and_then(|v| v.first())
                .and_then(|v| u16::try_from(*v).ok()),
        );
        let (size, offset) = match fields.get(&18).map(Vec::as_slice) {
            Some([size, offset]) => (*size, *offset),
            _ => return Err(head_error()),
        };
        let size = usize::try_from(size).map_err(|_| head_error())?;
        let offset = usize::try_from(offset).map_err(|_| head_error())?;
        let end = offset.checked_add(size).ok_or_else(head_error)?;
        let private = data.get(offset..end).ok_or_else(head_error)?;
        let (subrs, _) = local_subrs_from_private(data, offset, private)?;
        privates.push(private);
        local_subrs.push(subrs);
    }
    Ok((
        read_fd_select(data, fdselect_off, num_glyphs)?,
        privates,
        local_subrs,
        fd_font_names,
    ))
}

fn parse_name_keyed_layout<'a>(
    data: &'a [u8],
    top_fields: &BTreeMap<u16, Vec<i32>>,
    num_glyphs: usize,
) -> Result<FdLayout<'a>> {
    let (size, offset) = match top_fields.get(&18).map(Vec::as_slice) {
        Some([size, offset]) => (*size, *offset),
        _ => return Err(head_error()),
    };
    let size = usize::try_from(size).map_err(|_| head_error())?;
    let offset = usize::try_from(offset).map_err(|_| head_error())?;
    let end = offset.checked_add(size).ok_or_else(head_error)?;
    let private = data.get(offset..end).ok_or_else(head_error)?;
    let (subrs, _) = local_subrs_from_private(data, offset, private)?;
    Ok((
        vec![0_u8; num_glyphs],
        vec![private],
        vec![subrs],
        vec![None],
    ))
}

fn sid_field(fields: &BTreeMap<u16, Vec<i32>>, op: u16) -> Option<u16> {
    fields
        .get(&op)
        .and_then(|v| v.first())
        .and_then(|v| u16::try_from(*v).ok())
}

fn local_subrs_from_private<'a>(
    data: &'a [u8],
    private_offset: usize,
    private: &[u8],
) -> Result<(Vec<&'a [u8]>, Option<usize>)> {
    let fields = parse_dict_ops(private)?;
    let Some(rel) = fields.get(&19).and_then(|v| v.first().copied()) else {
        return Ok((Vec::new(), None));
    };
    let rel = usize::try_from(rel).map_err(|_| head_error())?;
    let at = private_offset.checked_add(rel).ok_or_else(head_error)?;
    let (_, subrs) = read_index(data, at)?;
    Ok((subrs, Some(at)))
}

fn read_fd_select(data: &[u8], offset: usize, num_glyphs: usize) -> Result<Vec<u8>> {
    let format = *data.get(offset).ok_or_else(head_error)?;
    match format {
        0 => {
            let end = offset.checked_add(1 + num_glyphs).ok_or_else(head_error)?;
            data.get(offset + 1..end)
                .map(Vec::from)
                .ok_or_else(head_error)
        }
        3 => {
            let n_ranges = usize::from(read_u16(data, offset + 1)?);
            let mut fds = vec![0_u8; num_glyphs];
            for range in 0..n_ranges {
                let at = offset + 3 + range * 3;
                let first = usize::from(read_u16(data, at)?);
                let fd = *data.get(at + 2).ok_or_else(head_error)?;
                let next = if range + 1 < n_ranges {
                    usize::from(read_u16(data, at + 3)?)
                } else {
                    usize::from(read_u16(data, offset + 3 + n_ranges * 3)?)
                };
                if next > num_glyphs || first > next {
                    return Err(head_error());
                }
                fds[first..next].fill(fd);
            }
            Ok(fds)
        }
        _ => Err(head_error()),
    }
}

enum CharEnd {
    Return,
    EndChar,
}

fn desubroutinize(
    charstring: &[u8],
    global_subrs: &[&[u8]],
    local_subrs: &[&[u8]],
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut stack = Vec::new();
    let mut hint_count = 0_u16;
    let mut hint_mask_bytes = 0_u16;
    desubroutinize_inner(
        charstring,
        global_subrs,
        local_subrs,
        &mut out,
        &mut stack,
        &mut hint_count,
        &mut hint_mask_bytes,
        0,
    )?;
    if out.last() != Some(&ENDCHAR) {
        out.push(ENDCHAR);
    }
    Ok(out)
}

fn desubroutinize_inner(
    charstring: &[u8],
    global_subrs: &[&[u8]],
    local_subrs: &[&[u8]],
    out: &mut Vec<u8>,
    stack: &mut Vec<i32>,
    hint_count: &mut u16,
    hint_mask_bytes: &mut u16,
    depth: u8,
) -> Result<CharEnd> {
    if depth > 64 {
        return Err(head_error());
    }
    let g_bias = subr_bias(global_subrs.len());
    let l_bias = subr_bias(local_subrs.len());
    let mut i = 0;
    while i < charstring.len() {
        let b0 = charstring[i];
        if matches!(b0, 28 | 32..=255) {
            let start = i;
            let (next, number) = read_cs_number(charstring, i)?;
            i = next;
            stack.push(number);
            out.extend_from_slice(&charstring[start..i]);
            continue;
        }
        i += 1;
        if b0 == ESCAPE {
            let b1 = *charstring.get(i).ok_or_else(head_error)?;
            i += 1;
            out.push(ESCAPE);
            out.push(b1);
            stack.clear();
            continue;
        }
        match b0 {
            CALLSUBR => {
                let biased = stack.pop().ok_or_else(head_error)?;
                pop_last_number_encoding(out)?;
                let index = unbiased(biased, l_bias)?;
                let subr = *local_subrs.get(index).ok_or_else(head_error)?;
                if matches!(
                    desubroutinize_inner(
                        subr,
                        global_subrs,
                        local_subrs,
                        out,
                        stack,
                        hint_count,
                        hint_mask_bytes,
                        depth + 1,
                    )?,
                    CharEnd::EndChar
                ) {
                    return Ok(CharEnd::EndChar);
                }
            }
            CALLGSUBR => {
                let biased = stack.pop().ok_or_else(head_error)?;
                pop_last_number_encoding(out)?;
                let index = unbiased(biased, g_bias)?;
                let subr = *global_subrs.get(index).ok_or_else(head_error)?;
                if matches!(
                    desubroutinize_inner(
                        subr,
                        global_subrs,
                        local_subrs,
                        out,
                        stack,
                        hint_count,
                        hint_mask_bytes,
                        depth + 1,
                    )?,
                    CharEnd::EndChar
                ) {
                    return Ok(CharEnd::EndChar);
                }
            }
            RETURN => return Ok(CharEnd::Return),
            ENDCHAR => {
                out.push(ENDCHAR);
                return Ok(CharEnd::EndChar);
            }
            HSTEM | VSTEM | HSTEMHM | VSTEMHM => {
                *hint_count =
                    hint_count.saturating_add(u16::try_from(stack.len() / 2).unwrap_or(0));
                out.push(b0);
                stack.clear();
            }
            HINTMASK | CNTRMASK => {
                out.push(b0);
                if *hint_mask_bytes == 0 {
                    *hint_count =
                        hint_count.saturating_add(u16::try_from(stack.len() / 2).unwrap_or(0));
                    *hint_mask_bytes = hint_count.div_ceil(8);
                }
                let n = usize::from(*hint_mask_bytes);
                let end = i.checked_add(n).ok_or_else(head_error)?;
                let mask = charstring.get(i..end).ok_or_else(head_error)?;
                out.extend_from_slice(mask);
                i = end;
                stack.clear();
            }
            _ => {
                out.push(b0);
                stack.clear();
            }
        }
    }
    Ok(CharEnd::Return)
}

fn subr_bias(count: usize) -> i32 {
    if count < 1240 {
        107
    } else if count < 33900 {
        1131
    } else {
        32768
    }
}

fn unbiased(biased: i32, bias: i32) -> Result<usize> {
    let index = biased.checked_add(bias).ok_or_else(head_error)?;
    usize::try_from(index).map_err(|_| head_error())
}

fn read_cs_number(data: &[u8], i: usize) -> Result<(usize, i32)> {
    let b0 = *data.get(i).ok_or_else(head_error)?;
    if b0 == 28 {
        let v = i16::from_be_bytes([
            *data.get(i + 1).ok_or_else(head_error)?,
            *data.get(i + 2).ok_or_else(head_error)?,
        ]);
        return Ok((i + 3, i32::from(v)));
    }
    if b0 == 255 {
        let v = i32::from_be_bytes([
            *data.get(i + 1).ok_or_else(head_error)?,
            *data.get(i + 2).ok_or_else(head_error)?,
            *data.get(i + 3).ok_or_else(head_error)?,
            *data.get(i + 4).ok_or_else(head_error)?,
        ]);
        return Ok((i + 5, v));
    }
    if (32..=246).contains(&b0) {
        return Ok((i + 1, i32::from(b0) - 139));
    }
    if (247..=250).contains(&b0) {
        let b1 = i32::from(*data.get(i + 1).ok_or_else(head_error)?);
        return Ok((i + 2, (i32::from(b0) - 247) * 256 + b1 + 108));
    }
    if (251..=254).contains(&b0) {
        let b1 = i32::from(*data.get(i + 1).ok_or_else(head_error)?);
        return Ok((i + 2, -(i32::from(b0) - 251) * 256 - b1 - 108));
    }
    Err(head_error())
}

fn pop_last_number_encoding(out: &mut Vec<u8>) -> Result<()> {
    if out.is_empty() {
        return Err(head_error());
    }
    // 2-byte encodings (247..=254, x) may end with 0xFF, so check those before
    // treating a trailing 0xFF as an incomplete fixed.
    if out.len() >= 2 {
        let b = out[out.len() - 2];
        if (247..=254).contains(&b) {
            out.truncate(out.len() - 2);
            return Ok(());
        }
        if b == 28 && out.len() >= 3 {
            out.truncate(out.len() - 3);
            return Ok(());
        }
    }
    if out.len() >= 5 && out[out.len() - 5] == 255 {
        out.truncate(out.len() - 5);
        return Ok(());
    }
    let last = *out.last().unwrap();
    if (32..=246).contains(&last) {
        out.pop();
        return Ok(());
    }
    Err(head_error())
}

fn read_index(data: &[u8], offset: usize) -> Result<(usize, Vec<&[u8]>)> {
    if offset + 2 > data.len() {
        return Err(head_error());
    }
    let count = usize::from(read_u16(data, offset)?);
    if count == 0 {
        return Ok((offset + 2, Vec::new()));
    }
    let off_size = usize::from(*data.get(offset + 2).ok_or_else(head_error)?);
    if !(1..=4).contains(&off_size) {
        return Err(head_error());
    }
    let offsets_len = (count + 1).checked_mul(off_size).ok_or_else(head_error)?;
    let offsets_at = offset + 3;
    let data_at = offsets_at.checked_add(offsets_len).ok_or_else(head_error)?;
    let mut offsets = Vec::with_capacity(count + 1);
    for index in 0..=count {
        let at = offsets_at + index * off_size;
        offsets.push(read_offset(data, at, off_size)?);
    }
    if offsets.first().copied() != Some(1) {
        return Err(head_error());
    }
    let last = *offsets.last().unwrap();
    let end = data_at.checked_add(last - 1).ok_or_else(head_error)?;
    if end > data.len() {
        return Err(head_error());
    }
    let mut objects = Vec::with_capacity(count);
    for window in offsets.windows(2) {
        let start = data_at + window[0] - 1;
        let stop = data_at + window[1] - 1;
        if stop < start || stop > data.len() {
            return Err(head_error());
        }
        objects.push(&data[start..stop]);
    }
    Ok((end, objects))
}

fn read_offset(data: &[u8], at: usize, off_size: usize) -> Result<usize> {
    let bytes = data.get(at..at + off_size).ok_or_else(head_error)?;
    let mut value = 0_usize;
    for byte in bytes {
        value = value
            .checked_mul(256)
            .and_then(|v| v.checked_add(usize::from(*byte)))
            .ok_or_else(head_error)?;
    }
    Ok(value)
}

fn write_index(out: &mut Vec<u8>, objects: &[Vec<u8>]) -> Result<()> {
    let count = u16::try_from(objects.len()).map_err(|_| head_error())?;
    out.extend_from_slice(&count.to_be_bytes());
    if objects.is_empty() {
        return Ok(());
    }
    let total: usize = objects.iter().map(Vec::len).sum();
    let off_size = off_size_for(total + 1)?;
    out.push(off_size);
    let mut offset = 1_usize;
    write_offset(out, offset, off_size)?;
    for object in objects {
        offset = offset.checked_add(object.len()).ok_or_else(head_error)?;
        write_offset(out, offset, off_size)?;
    }
    for object in objects {
        out.extend_from_slice(object);
    }
    Ok(())
}

fn off_size_for(max_offset: usize) -> Result<u8> {
    if max_offset <= 0xff {
        Ok(1)
    } else if max_offset <= 0xffff {
        Ok(2)
    } else if max_offset <= 0xff_ffff {
        Ok(3)
    } else if max_offset <= 0xffff_ffff {
        Ok(4)
    } else {
        Err(head_error())
    }
}

fn write_offset(out: &mut Vec<u8>, value: usize, off_size: u8) -> Result<()> {
    let bytes = match off_size {
        1 => {
            out.push(u8::try_from(value).map_err(|_| head_error())?);
            return Ok(());
        }
        2 => u16::try_from(value)
            .map_err(|_| head_error())?
            .to_be_bytes()
            .to_vec(),
        3 => {
            let v = u32::try_from(value).map_err(|_| head_error())?;
            vec![
                u8::try_from((v >> 16) & 0xff).unwrap(),
                u8::try_from((v >> 8) & 0xff).unwrap(),
                u8::try_from(v & 0xff).unwrap(),
            ]
        }
        4 => u32::try_from(value)
            .map_err(|_| head_error())?
            .to_be_bytes()
            .to_vec(),
        _ => return Err(head_error()),
    };
    out.extend_from_slice(&bytes);
    Ok(())
}

fn write_identity_charset(out: &mut Vec<u8>, num_glyphs: usize) -> Result<()> {
    if num_glyphs == 0 {
        return Err(head_error());
    }
    if num_glyphs == 1 {
        out.push(0);
        return Ok(());
    }
    out.push(2);
    out.extend_from_slice(&1_u16.to_be_bytes());
    let n_left = u16::try_from(num_glyphs - 2).map_err(|_| head_error())?;
    out.extend_from_slice(&n_left.to_be_bytes());
    Ok(())
}

fn write_fd_select(
    out: &mut Vec<u8>,
    num_glyphs: usize,
    kept: &BTreeSet<u16>,
    parsed: &ParsedCff<'_>,
    fd_map: &BTreeMap<u8, u8>,
) -> Result<()> {
    let mut fds = Vec::with_capacity(num_glyphs);
    for gid in 0..num_glyphs {
        let gid_u = u16::try_from(gid).map_err(|_| head_error())?;
        let old = if kept.contains(&gid_u) {
            parsed.fd_for(gid_u)?
        } else {
            0
        };
        let new = *fd_map.get(&old).unwrap_or(&0);
        fds.push(new);
    }
    out.push(3);
    let mut ranges = Vec::new();
    let mut start = 0_u16;
    let mut current = fds[0];
    for (gid, &fd) in fds.iter().enumerate().skip(1) {
        if fd != current {
            ranges.push((start, current));
            start = u16::try_from(gid).map_err(|_| head_error())?;
            current = fd;
        }
    }
    ranges.push((start, current));
    let n = u16::try_from(ranges.len()).map_err(|_| head_error())?;
    out.extend_from_slice(&n.to_be_bytes());
    for (first, fd) in ranges {
        out.extend_from_slice(&first.to_be_bytes());
        out.push(fd);
    }
    out.extend_from_slice(
        &u16::try_from(num_glyphs)
            .map_err(|_| head_error())?
            .to_be_bytes(),
    );
    Ok(())
}

fn strip_subrs_op(private: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut stack_bytes: Vec<u8> = Vec::new();
    while i < private.len() {
        let b0 = private[i];
        if matches!(b0, 28 | 29 | 30 | 32..=254) {
            let (next, raw) = take_dict_number(private, i)?;
            stack_bytes.extend_from_slice(raw);
            i = next;
            continue;
        }
        i += 1;
        if b0 == ESCAPE {
            let b1 = *private.get(i).ok_or_else(head_error)?;
            i += 1;
            out.append(&mut stack_bytes);
            out.push(ESCAPE);
            out.push(b1);
            continue;
        }
        if b0 == 19 {
            stack_bytes.clear();
            continue;
        }
        out.append(&mut stack_bytes);
        out.push(b0);
    }
    Ok(out)
}

fn take_dict_number(data: &[u8], i: usize) -> Result<(usize, &[u8])> {
    let b0 = *data.get(i).ok_or_else(head_error)?;
    if b0 == 28 {
        return Ok((i + 3, data.get(i..i + 3).ok_or_else(head_error)?));
    }
    if b0 == 29 {
        return Ok((i + 5, data.get(i..i + 5).ok_or_else(head_error)?));
    }
    if b0 == 30 {
        let mut j = i + 1;
        loop {
            let b = *data.get(j).ok_or_else(head_error)?;
            j += 1;
            if b & 0xf0 == 0xf0 || b & 0x0f == 0x0f {
                break;
            }
        }
        return Ok((j, data.get(i..j).ok_or_else(head_error)?));
    }
    if (32..=246).contains(&b0) {
        return Ok((i + 1, data.get(i..i + 1).ok_or_else(head_error)?));
    }
    if (247..=254).contains(&b0) {
        return Ok((i + 2, data.get(i..i + 2).ok_or_else(head_error)?));
    }
    Err(head_error())
}

fn parse_dict_ops(data: &[u8]) -> Result<BTreeMap<u16, Vec<i32>>> {
    let mut fields = BTreeMap::new();
    let mut stack = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i];
        if matches!(b0, 28 | 29 | 30 | 32..=254) {
            let (next, value) = read_dict_number(data, i)?;
            stack.push(value);
            i = next;
            continue;
        }
        i += 1;
        let op = if b0 == ESCAPE {
            let b1 = *data.get(i).ok_or_else(head_error)?;
            i += 1;
            1200 + u16::from(b1)
        } else {
            u16::from(b0)
        };
        fields.insert(op, std::mem::take(&mut stack));
    }
    Ok(fields)
}

fn read_dict_number(data: &[u8], i: usize) -> Result<(usize, i32)> {
    let b0 = *data.get(i).ok_or_else(head_error)?;
    if b0 == 28 {
        let v = i16::from_be_bytes([
            *data.get(i + 1).ok_or_else(head_error)?,
            *data.get(i + 2).ok_or_else(head_error)?,
        ]);
        return Ok((i + 3, i32::from(v)));
    }
    if b0 == 29 {
        let v = i32::from_be_bytes([
            *data.get(i + 1).ok_or_else(head_error)?,
            *data.get(i + 2).ok_or_else(head_error)?,
            *data.get(i + 3).ok_or_else(head_error)?,
            *data.get(i + 4).ok_or_else(head_error)?,
        ]);
        return Ok((i + 5, v));
    }
    if b0 == 30 {
        let mut j = i + 1;
        loop {
            let b = *data.get(j).ok_or_else(head_error)?;
            j += 1;
            if b & 0xf0 == 0xf0 || b & 0x0f == 0x0f {
                break;
            }
        }
        return Ok((j, 0));
    }
    if (32..=246).contains(&b0) {
        return Ok((i + 1, i32::from(b0) - 139));
    }
    if (247..=250).contains(&b0) {
        let b1 = i32::from(*data.get(i + 1).ok_or_else(head_error)?);
        return Ok((i + 2, (i32::from(b0) - 247) * 256 + b1 + 108));
    }
    if (251..=254).contains(&b0) {
        let b1 = i32::from(*data.get(i + 1).ok_or_else(head_error)?);
        return Ok((i + 2, -(i32::from(b0) - 251) * 256 - b1 - 108));
    }
    Err(head_error())
}

fn write_dict_int(out: &mut Vec<u8>, value: i32) {
    if (-107..=107).contains(&value) {
        out.push(u8::try_from(value + 139).unwrap());
    } else if (108..=1131).contains(&value) {
        let v = value - 108;
        out.push(u8::try_from((v >> 8) + 247).unwrap());
        out.push(u8::try_from(v & 0xff).unwrap());
    } else if (-1131..=-108).contains(&value) {
        let v = -value - 108;
        out.push(u8::try_from((v >> 8) + 251).unwrap());
        out.push(u8::try_from(v & 0xff).unwrap());
    } else if (i32::from(i16::MIN)..=i32::from(i16::MAX)).contains(&value) {
        out.push(28);
        out.extend_from_slice(&i16::try_from(value).unwrap().to_be_bytes());
    } else {
        write_dict_int5(out, value);
    }
}

fn write_dict_int5(out: &mut Vec<u8>, value: i32) {
    out.push(29);
    out.extend_from_slice(&value.to_be_bytes());
}

fn write_dict_int5_placeholder(out: &mut Vec<u8>) {
    write_dict_int5(out, 0);
}

fn write_dict_sid(out: &mut Vec<u8>, sid: Option<u16>, op: u8) {
    if let Some(sid) = sid {
        write_dict_int(out, i32::from(sid));
        out.push(op);
    }
}

fn extract_operator_bytes(dict: &[u8], op: u16) -> Result<Option<Vec<u8>>> {
    let mut i = 0;
    let mut operand_start = 0_usize;
    let mut have_operands = false;
    while i < dict.len() {
        let b0 = dict[i];
        if matches!(b0, 28 | 29 | 30 | 32..=254) {
            if !have_operands {
                operand_start = i;
                have_operands = true;
            }
            let (next, _) = read_dict_number(dict, i)?;
            i = next;
            continue;
        }
        let start = if have_operands { operand_start } else { i };
        i += 1;
        let this_op = if b0 == ESCAPE {
            let b1 = *dict.get(i).ok_or_else(head_error)?;
            i += 1;
            1200 + u16::from(b1)
        } else {
            u16::from(b0)
        };
        if this_op == op {
            return Ok(Some(dict[start..i].to_vec()));
        }
        have_operands = false;
    }
    Ok(None)
}

struct StringTable {
    strings: Vec<Vec<u8>>,
}

impl StringTable {
    fn from_existing(existing: &[&[u8]]) -> Self {
        Self {
            strings: existing.iter().map(|s| s.to_vec()).collect(),
        }
    }

    fn intern(&mut self, bytes: &[u8]) -> u16 {
        const STANDARD: u16 = 391;
        if let Some(index) = self.strings.iter().position(|s| s.as_slice() == bytes) {
            return STANDARD + u16::try_from(index).unwrap_or(0);
        }
        self.strings.push(bytes.to_vec());
        STANDARD + u16::try_from(self.strings.len() - 1).unwrap_or(0)
    }

    fn bytes(&self) -> Vec<Vec<u8>> {
        self.strings.clone()
    }
}

fn patch_font_dict_privates(
    out: &mut [u8],
    index_start: usize,
    font_dicts: &[Vec<u8>],
    patch_sites: &[usize],
    private_meta: &[(usize, usize)],
) -> Result<()> {
    let count = font_dicts.len();
    if count == 0 {
        return Ok(());
    }
    let off_size = usize::from(*out.get(index_start + 2).ok_or_else(head_error)?);
    let data_start = index_start + 3 + (count + 1) * off_size;
    for (idx, site) in patch_sites.iter().enumerate() {
        let (len, offset) = private_meta[idx];
        let at =
            data_start + read_offset(out, index_start + 3 + idx * off_size, off_size)? - 1 + site;
        write_i32_be_at(out, at + 1, i32::try_from(len).map_err(|_| head_error())?)?;
        write_i32_be_at(
            out,
            at + 6,
            i32::try_from(offset).map_err(|_| head_error())?,
        )?;
    }
    Ok(())
}

fn patch_top_dict_offsets(
    out: &mut [u8],
    sites: &TopDictSites,
    values: &TopDictValues,
) -> Result<()> {
    let name_count = usize::from(read_u16(out, 4)?);
    let after_name = if name_count == 0 {
        6
    } else {
        let off_size = usize::from(out[6]);
        let last = read_offset(out, 7 + name_count * off_size, off_size)?;
        7 + (name_count + 1) * off_size + last - 1
    };
    let top_count = usize::from(read_u16(out, after_name)?);
    if top_count == 0 {
        return Err(head_error());
    }
    let off_size = usize::from(*out.get(after_name + 2).ok_or_else(head_error)?);
    let top_data = after_name + 3 + (top_count + 1) * off_size;
    let top_obj = top_data + read_offset(out, after_name + 3, off_size)? - 1;

    let mut patch = |rel: usize, value: usize| -> Result<()> {
        write_i32_be_at(
            out,
            top_obj + rel + 1,
            i32::try_from(value).map_err(|_| head_error())?,
        )
    };
    patch(sites.charset, values.charset)?;
    patch(sites.char_strings, values.char_strings)?;
    patch(sites.fd_array, values.fd_array)?;
    patch(sites.fd_select, values.fd_select)?;
    Ok(())
}

fn write_i32_be_at(out: &mut [u8], at: usize, value: i32) -> Result<()> {
    let bytes = value.to_be_bytes();
    let slot = out.get_mut(at..at + 4).ok_or_else(head_error)?;
    slot.copy_from_slice(&bytes);
    Ok(())
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_be_bytes([
        *data.get(offset).ok_or_else(head_error)?,
        *data.get(offset + 1).ok_or_else(head_error)?,
    ]))
}

fn head_error() -> crate::error::Error {
    font_unusable(FontReason::Head)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use std::collections::BTreeMap;

    #[test]
    fn subsets_name_keyed_cff_and_is_deterministic() {
        let cff = fixture_cff();
        let used = used_map([(1, '_')]);
        let first = subset_cff(&cff, &used).unwrap();
        let second = subset_cff(&cff, &used).unwrap();
        assert_eq!(first, second);
        assert!(first.len() < cff.len() || first.len() <= cff.len() + 64);
        assert_eq!(first[0], 1);
        let parsed = parse_cff(&first).unwrap();
        assert_eq!(parsed.num_glyphs, 2);
        assert!(parsed.global_subrs.is_empty());
    }

    #[test]
    fn damaged_cff_is_head_not_flatten() {
        let err = subset_cff(b"not a cff", &used_map([(1, 'A')])).unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::FontNotConfigured {
                reason: FontReason::Head,
            }
        );
        assert!(std::error::Error::source(&err).is_none());
    }

    #[test]
    fn rejects_gid_past_charstrings() {
        let cff = fixture_cff();
        let err = subset_cff(&cff, &used_map([(9, 'A')])).unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::FontNotConfigured {
                reason: FontReason::Head,
            }
        );
    }

    fn used_map<const N: usize>(entries: [(u16, char); N]) -> BTreeMap<u16, (char, f32)> {
        entries
            .into_iter()
            .map(|(gid, ch)| (gid, (ch, 500.0)))
            .collect()
    }

    #[test]
    fn subsets_source_han_cid_when_present() {
        let path = std::path::Path::new(
            "/mnt/data/apps/hcsw-entrance-guard-hongkong-app/public/fonts/SourceHanSerifHC/SourceHanSerifHC-ExtraLight.otf",
        );
        if !path.is_file() {
            return;
        }
        let sfnt = std::fs::read(path).unwrap();
        let count = usize::from(u16::from_be_bytes([sfnt[4], sfnt[5]]));
        let mut cff = None;
        for index in 0..count {
            let at = 12 + index * 16;
            if &sfnt[at..at + 4] == b"CFF " {
                let off = u32::from_be_bytes(sfnt[at + 8..at + 12].try_into().unwrap()) as usize;
                let len = u32::from_be_bytes(sfnt[at + 12..at + 16].try_into().unwrap()) as usize;
                cff = Some(sfnt[off..off + len].to_vec());
                break;
            }
        }
        let cff = cff.expect("CFF");
        let used = used_map([(1, '、'), (100, 'x'), (5000, '中')]);
        let out = subset_cff(&cff, &used).expect("subset");
        let again = subset_cff(&cff, &used).unwrap();
        assert_eq!(out, again);
        assert!(
            out.len() < cff.len() / 100,
            "subset {} vs {}",
            out.len(),
            cff.len()
        );
        let parsed = parse_cff(&out).unwrap();
        assert_eq!(parsed.num_glyphs, 5001);
        assert!(parsed.global_subrs.is_empty());
        assert!(parsed.charstrings[100].len() > 1);
        assert_eq!(parsed.charstrings[2], vec![ENDCHAR]);
    }

    fn fixture_cff() -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/minimal_cff1.otf");
        let sfnt = std::fs::read(path).unwrap();
        let count = usize::from(u16::from_be_bytes([sfnt[4], sfnt[5]]));
        for index in 0..count {
            let at = 12 + index * 16;
            if &sfnt[at..at + 4] == b"CFF " {
                let off = u32::from_be_bytes(sfnt[at + 8..at + 12].try_into().unwrap()) as usize;
                let len = u32::from_be_bytes(sfnt[at + 12..at + 16].try_into().unwrap()) as usize;
                return sfnt[off..off + len].to_vec();
            }
        }
        panic!("CFF table missing");
    }
}
