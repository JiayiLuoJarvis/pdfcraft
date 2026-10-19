use std::collections::{BTreeMap, BTreeSet};

use crate::error::{font_unusable, FontReason, Result};
use crate::font::checksum;

type TableTag = [u8; 4];
type Tables = BTreeMap<TableTag, Vec<u8>>;

const ARG_1_AND_2_ARE_WORDS: u16 = 1 << 0;
const WE_HAVE_A_SCALE: u16 = 1 << 3;
const MORE_COMPONENTS: u16 = 1 << 5;
const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 1 << 6;
const WE_HAVE_A_TWO_BY_TWO: u16 = 1 << 7;

#[derive(Clone, Copy)]
enum TablePolicy {
    Copy,
    Rewrite,
    Drop,
}

#[derive(Clone, Copy)]
struct CmapGroup {
    start_scalar: u32,
    end_scalar: u32,
    start_gid: u16,
}

pub(crate) fn subset_sfnt(sfnt: &[u8], used: &BTreeMap<u16, (char, f32)>) -> Result<Vec<u8>> {
    let (scaler, tables) = parse_sfnt(sfnt)?;
    let head = required_table(&tables, *b"head")?;
    let maxp = required_table(&tables, *b"maxp")?;
    let loca = required_table(&tables, *b"loca")?;
    let glyf = required_table(&tables, *b"glyf")?;
    required_table(&tables, *b"hhea")?;
    required_table(&tables, *b"hmtx")?;

    let num_glyphs = usize::from(read_u16(maxp, 4)?);
    if num_glyphs == 0 || head.len() < 54 {
        return Err(head_error());
    }
    let source_loca_format = read_i16(head, 50)?;
    if !matches!(source_loca_format, 0 | 1) {
        return Err(head_error());
    }
    let source_offsets = parse_loca(loca, num_glyphs, source_loca_format)?;
    validate_offsets(&source_offsets, glyf.len())?;

    let mut kept = BTreeSet::new();
    kept.insert(0);
    for gid in used.keys().copied() {
        if usize::from(gid) >= num_glyphs {
            return Err(head_error());
        }
        kept.insert(gid);
    }
    close_composites(&mut kept, &source_offsets, glyf, num_glyphs)?;

    let (subset_glyf, subset_offsets) = build_glyf(&kept, &source_offsets, glyf, num_glyphs)?;
    let loca_format = i16::from(source_loca_format == 1 || !offsets_fit_short(&subset_offsets));
    let subset_loca = build_loca(&subset_offsets, loca_format)?;
    let mut subset_head = head.to_vec();
    subset_head[50..52].copy_from_slice(&loca_format.to_be_bytes());
    let subset_cmap = build_cmap(used)?;

    let mut output = BTreeMap::new();
    for (tag, data) in &tables {
        if matches!(table_policy(*tag), TablePolicy::Copy) {
            output.insert(*tag, data.clone());
        }
    }
    output.insert(*b"head", subset_head);
    output.insert(*b"loca", subset_loca);
    output.insert(*b"glyf", subset_glyf);
    output.insert(*b"cmap", subset_cmap);
    write_sfnt(scaler, &output)
}

fn table_policy(tag: TableTag) -> TablePolicy {
    match &tag {
        b"head" | b"loca" | b"glyf" | b"cmap" => TablePolicy::Rewrite,
        b"maxp" | b"hhea" | b"hmtx" | b"name" | b"OS/2" | b"post" | b"cvt " | b"fpgm" | b"prep"
        | b"gasp" => TablePolicy::Copy,
        _ => TablePolicy::Drop,
    }
}

fn parse_sfnt(sfnt: &[u8]) -> Result<(TableTag, Tables)> {
    if sfnt.len() < 12 {
        return Err(head_error());
    }
    let mut scaler = [0; 4];
    scaler.copy_from_slice(&sfnt[..4]);
    let count = usize::from(read_u16(sfnt, 4)?);
    let directory_len = count
        .checked_mul(16)
        .and_then(|len| 12_usize.checked_add(len))
        .ok_or_else(head_error)?;
    if directory_len > sfnt.len() {
        return Err(head_error());
    }
    let mut tables = BTreeMap::new();
    let mut ranges = Vec::new();
    for index in 0..count {
        let record = 12 + index * 16;
        let mut tag = [0; 4];
        tag.copy_from_slice(&sfnt[record..record + 4]);
        let offset = usize::try_from(read_u32(sfnt, record + 8)?).map_err(|_| head_error())?;
        let length = usize::try_from(read_u32(sfnt, record + 12)?).map_err(|_| head_error())?;
        let end = offset.checked_add(length).ok_or_else(head_error)?;
        if offset < directory_len
            || offset % 4 != 0
            || end > sfnt.len()
            || tables.insert(tag, sfnt[offset..end].to_vec()).is_some()
        {
            return Err(head_error());
        }
        if length != 0 {
            ranges.push((offset, end));
        }
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(head_error());
    }
    Ok((scaler, tables))
}

fn required_table(tables: &Tables, tag: TableTag) -> Result<&[u8]> {
    tables.get(&tag).map(Vec::as_slice).ok_or_else(head_error)
}

fn parse_loca(data: &[u8], num_glyphs: usize, format: i16) -> Result<Vec<usize>> {
    let count = num_glyphs.checked_add(1).ok_or_else(head_error)?;
    let item_len = if format == 0 { 2 } else { 4 };
    let needed = count.checked_mul(item_len).ok_or_else(head_error)?;
    if data.len() < needed {
        return Err(head_error());
    }
    let mut offsets = Vec::with_capacity(count);
    for index in 0..count {
        let at = index * item_len;
        let offset = if format == 0 {
            usize::from(read_u16(data, at)?) * 2
        } else {
            usize::try_from(read_u32(data, at)?).map_err(|_| head_error())?
        };
        offsets.push(offset);
    }
    Ok(offsets)
}

fn validate_offsets(offsets: &[usize], glyf_len: usize) -> Result<()> {
    if offsets
        .last()
        .copied()
        .is_none_or(|offset| offset > glyf_len)
        || offsets.windows(2).any(|pair| pair[0] > pair[1])
    {
        return Err(head_error());
    }
    Ok(())
}

fn close_composites(
    kept: &mut BTreeSet<u16>,
    offsets: &[usize],
    glyf: &[u8],
    num_glyphs: usize,
) -> Result<()> {
    let mut pending: Vec<u16> = kept.iter().copied().collect();
    let mut next = 0;
    while next < pending.len() {
        let gid = pending[next];
        next += 1;
        let data = glyph_data(offsets, glyf, gid)?;
        if data.is_empty() {
            continue;
        }
        if data.len() < 10 {
            return Err(head_error());
        }
        if read_i16(data, 0)? >= 0 {
            continue;
        }
        let mut cursor = 10;
        loop {
            let flags = read_u16(data, cursor)?;
            let component = read_u16(data, cursor + 2)?;
            if usize::from(component) >= num_glyphs {
                return Err(head_error());
            }
            cursor = cursor.checked_add(4).ok_or_else(head_error)?;
            cursor = cursor
                .checked_add(if flags & ARG_1_AND_2_ARE_WORDS != 0 {
                    4
                } else {
                    2
                })
                .ok_or_else(head_error)?;
            let transform_len = if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
                8
            } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
                4
            } else if flags & WE_HAVE_A_SCALE != 0 {
                2
            } else {
                0
            };
            cursor = cursor.checked_add(transform_len).ok_or_else(head_error)?;
            if cursor > data.len() {
                return Err(head_error());
            }
            if kept.insert(component) {
                pending.push(component);
            }
            if flags & MORE_COMPONENTS == 0 {
                break;
            }
        }
    }
    Ok(())
}

fn glyph_data<'a>(offsets: &[usize], glyf: &'a [u8], gid: u16) -> Result<&'a [u8]> {
    let index = usize::from(gid);
    let start = offsets.get(index).copied().ok_or_else(head_error)?;
    let end = offsets.get(index + 1).copied().ok_or_else(head_error)?;
    glyf.get(start..end).ok_or_else(head_error)
}

fn build_glyf(
    kept: &BTreeSet<u16>,
    source_offsets: &[usize],
    source_glyf: &[u8],
    num_glyphs: usize,
) -> Result<(Vec<u8>, Vec<usize>)> {
    let mut glyf = Vec::new();
    let mut offsets = Vec::with_capacity(num_glyphs + 1);
    for gid in 0..num_glyphs {
        offsets.push(glyf.len());
        let gid = u16::try_from(gid).map_err(|_| head_error())?;
        if kept.contains(&gid) {
            glyf.extend_from_slice(glyph_data(source_offsets, source_glyf, gid)?);
            if glyf.len() % 2 != 0 {
                glyf.push(0);
            }
        }
    }
    offsets.push(glyf.len());
    while glyf.len() % 4 != 0 {
        glyf.push(0);
    }
    Ok((glyf, offsets))
}

fn offsets_fit_short(offsets: &[usize]) -> bool {
    offsets
        .iter()
        .all(|offset| offset % 2 == 0 && u16::try_from(offset / 2).is_ok())
}

fn build_loca(offsets: &[usize], format: i16) -> Result<Vec<u8>> {
    let item_len = if format == 0 { 2 } else { 4 };
    let capacity = offsets.len().checked_mul(item_len).ok_or_else(head_error)?;
    let mut data = Vec::with_capacity(capacity);
    for offset in offsets {
        if format == 0 {
            let half = u16::try_from(offset / 2).map_err(|_| head_error())?;
            data.extend_from_slice(&half.to_be_bytes());
        } else {
            let offset = u32::try_from(*offset).map_err(|_| head_error())?;
            data.extend_from_slice(&offset.to_be_bytes());
        }
    }
    Ok(data)
}

fn build_cmap(used: &BTreeMap<u16, (char, f32)>) -> Result<Vec<u8>> {
    let mut by_scalar = BTreeMap::new();
    for (gid, (scalar, _)) in used {
        match by_scalar.insert(u32::from(*scalar), *gid) {
            Some(previous) if previous != *gid => return Err(head_error()),
            _ => {}
        }
    }
    let mappings: Vec<(u32, u16)> = by_scalar.into_iter().collect();
    let format12 = build_format12(&mappings)?;
    let format4 = mappings
        .iter()
        .all(|(scalar, _)| u16::try_from(*scalar).is_ok())
        .then(|| build_format4(&mappings))
        .transpose()?;
    let record_count = if format4.is_some() { 2_u16 } else { 1 };
    let header_len = 4_usize + usize::from(record_count) * 8;
    let mut cmap =
        Vec::with_capacity(header_len + format4.as_ref().map_or(0, Vec::len) + format12.len());
    push_u16(&mut cmap, 0);
    push_u16(&mut cmap, record_count);
    let mut offset = u32::try_from(header_len).map_err(|_| head_error())?;
    if let Some(table) = &format4 {
        push_u16(&mut cmap, 3);
        push_u16(&mut cmap, 1);
        push_u32(&mut cmap, offset);
        offset = offset
            .checked_add(u32::try_from(table.len()).map_err(|_| head_error())?)
            .ok_or_else(head_error)?;
    }
    push_u16(&mut cmap, 3);
    push_u16(&mut cmap, 10);
    push_u32(&mut cmap, offset);
    if let Some(table) = format4 {
        cmap.extend_from_slice(&table);
    }
    cmap.extend_from_slice(&format12);
    Ok(cmap)
}

fn cmap_groups(mappings: &[(u32, u16)]) -> Vec<CmapGroup> {
    let mut groups: Vec<CmapGroup> = Vec::new();
    for (scalar, gid) in mappings {
        if let Some(group) = groups.last_mut() {
            let gid_offset = group.end_scalar - group.start_scalar;
            if group.end_scalar.checked_add(1) == Some(*scalar)
                && u32::from(group.start_gid).checked_add(gid_offset + 1) == Some(u32::from(*gid))
            {
                group.end_scalar = *scalar;
                continue;
            }
        }
        groups.push(CmapGroup {
            start_scalar: *scalar,
            end_scalar: *scalar,
            start_gid: *gid,
        });
    }
    groups
}

fn build_format12(mappings: &[(u32, u16)]) -> Result<Vec<u8>> {
    let groups = cmap_groups(mappings);
    let length = groups
        .len()
        .checked_mul(12)
        .and_then(|value| value.checked_add(16))
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(head_error)?;
    let mut table = Vec::with_capacity(usize::try_from(length).map_err(|_| head_error())?);
    push_u16(&mut table, 12);
    push_u16(&mut table, 0);
    push_u32(&mut table, length);
    push_u32(&mut table, 0);
    push_u32(
        &mut table,
        u32::try_from(groups.len()).map_err(|_| head_error())?,
    );
    for group in groups {
        push_u32(&mut table, group.start_scalar);
        push_u32(&mut table, group.end_scalar);
        push_u32(&mut table, u32::from(group.start_gid));
    }
    Ok(table)
}

fn build_format4(mappings: &[(u32, u16)]) -> Result<Vec<u8>> {
    let bmp_mappings: Vec<(u32, u16)> = mappings
        .iter()
        .copied()
        .filter(|(scalar, _)| *scalar != u32::from(u16::MAX))
        .collect();
    let groups = cmap_groups(&bmp_mappings);
    let segment_count = groups.len().checked_add(1).ok_or_else(head_error)?;
    let segment_count_u16 = u16::try_from(segment_count).map_err(|_| head_error())?;
    let seg_count_x2 = segment_count_u16.checked_mul(2).ok_or_else(head_error)?;
    let length = 16_usize
        .checked_add(segment_count.checked_mul(8).ok_or_else(head_error)?)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(head_error)?;
    let (search_range, entry_selector, range_shift) = format4_search(segment_count_u16)?;
    let mut table = Vec::with_capacity(usize::from(length));
    push_u16(&mut table, 4);
    push_u16(&mut table, length);
    push_u16(&mut table, 0);
    push_u16(&mut table, seg_count_x2);
    push_u16(&mut table, search_range);
    push_u16(&mut table, entry_selector);
    push_u16(&mut table, range_shift);
    for group in &groups {
        push_u16(
            &mut table,
            u16::try_from(group.end_scalar).map_err(|_| head_error())?,
        );
    }
    push_u16(&mut table, u16::MAX);
    push_u16(&mut table, 0);
    for group in &groups {
        push_u16(
            &mut table,
            u16::try_from(group.start_scalar).map_err(|_| head_error())?,
        );
    }
    push_u16(&mut table, u16::MAX);
    for group in &groups {
        let delta = i64::from(group.start_gid) - i64::from(group.start_scalar);
        let delta = u16::try_from(delta.rem_euclid(1 << 16)).map_err(|_| head_error())?;
        push_u16(&mut table, delta);
    }
    push_u16(&mut table, 1);
    for _ in 0..segment_count {
        push_u16(&mut table, 0);
    }
    Ok(table)
}

fn format4_search(segment_count: u16) -> Result<(u16, u16, u16)> {
    let mut search_units = 1_u16;
    let mut entry_selector = 0_u16;
    while search_units
        .checked_mul(2)
        .is_some_and(|next| next <= segment_count)
    {
        search_units *= 2;
        entry_selector += 1;
    }
    let search_range = search_units.checked_mul(2).ok_or_else(head_error)?;
    let range_shift = segment_count
        .checked_mul(2)
        .and_then(|count| count.checked_sub(search_range))
        .ok_or_else(head_error)?;
    Ok((search_range, entry_selector, range_shift))
}

fn write_sfnt(scaler: TableTag, tables: &Tables) -> Result<Vec<u8>> {
    let count = u16::try_from(tables.len()).map_err(|_| head_error())?;
    if count == 0 {
        return Err(head_error());
    }
    let (search_range, entry_selector, range_shift) = sfnt_search(count)?;
    let directory_len = tables
        .len()
        .checked_mul(16)
        .and_then(|value| value.checked_add(12))
        .ok_or_else(head_error)?;
    let mut output = vec![0; directory_len];
    output[..4].copy_from_slice(&scaler);
    output[4..6].copy_from_slice(&count.to_be_bytes());
    output[6..8].copy_from_slice(&search_range.to_be_bytes());
    output[8..10].copy_from_slice(&entry_selector.to_be_bytes());
    output[10..12].copy_from_slice(&range_shift.to_be_bytes());

    for (index, (tag, data)) in tables.iter().enumerate() {
        while output.len() % 4 != 0 {
            output.push(0);
        }
        let offset = u32::try_from(output.len()).map_err(|_| head_error())?;
        let length = u32::try_from(data.len()).map_err(|_| head_error())?;
        let record = 12 + index * 16;
        output[record..record + 4].copy_from_slice(tag);
        output[record + 4..record + 8].copy_from_slice(&checksum(data).to_be_bytes());
        output[record + 8..record + 12].copy_from_slice(&offset.to_be_bytes());
        output[record + 12..record + 16].copy_from_slice(&length.to_be_bytes());
        output.extend_from_slice(data);
        while output.len() % 4 != 0 {
            output.push(0);
        }
    }
    Ok(output)
}

fn sfnt_search(table_count: u16) -> Result<(u16, u16, u16)> {
    let mut search_units = 1_u16;
    let mut entry_selector = 0_u16;
    while search_units
        .checked_mul(2)
        .is_some_and(|next| next <= table_count)
    {
        search_units *= 2;
        entry_selector += 1;
    }
    let search_range = search_units.checked_mul(16).ok_or_else(head_error)?;
    let range_shift = table_count
        .checked_mul(16)
        .and_then(|count| count.checked_sub(search_range))
        .ok_or_else(head_error)?;
    Ok((search_range, entry_selector, range_shift))
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    let bytes: [u8; 2] = data
        .get(offset..offset.checked_add(2).ok_or_else(head_error)?)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(head_error)?;
    Ok(u16::from_be_bytes(bytes))
}

fn read_i16(data: &[u8], offset: usize) -> Result<i16> {
    Ok(i16::from_be_bytes(read_u16(data, offset)?.to_be_bytes()))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32> {
    let bytes: [u8; 4] = data
        .get(offset..offset.checked_add(4).ok_or_else(head_error)?)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(head_error)?;
    Ok(u32::from_be_bytes(bytes))
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn head_error() -> crate::error::Error {
    font_unusable(FontReason::Head)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{build_cmap, parse_sfnt, subset_sfnt, write_sfnt};
    use crate::error::{ErrorKind, FontReason};
    use crate::font::{Face, FontFile, GlyphFault};

    #[test]
    fn keeps_only_used_simple_glyphs_and_notdef() {
        let source = test_font();
        let subset = subset_sfnt(&source, &used([(1, 'A')])).unwrap();
        let spans = loca_spans(&subset);
        assert!(spans[0] > 0);
        assert!(spans[1] > 0);
        assert_eq!(spans[2], 0);
        assert_eq!(spans[3], 0);
    }

    #[test]
    fn keeps_composite_components_recursively() {
        let source = test_font();
        let subset = subset_sfnt(&source, &used([(3, 'C')])).unwrap();
        let spans = loca_spans(&subset);
        assert!(spans[0] > 0);
        assert!(spans[1] > 0);
        assert!(spans[2] > 0);
        assert!(spans[3] > 0);
    }

    #[test]
    fn subset_preserves_cmap_glyph_ids_and_loads_as_a_face() {
        let source = test_font();
        let subset = subset_sfnt(&source, &used([(1, 'A')])).unwrap();
        let source_gid = match Face::load(&source, 0).unwrap().glyph('A') {
            Ok(glyph) => glyph.id,
            Err(GlyphFault::Missing(_)) => panic!("source font does not map A"),
            Err(GlyphFault::NoAdvance) => panic!("source font has no advance for A"),
        };
        let subset_gid = match Face::load(&subset, 0).unwrap().glyph('A') {
            Ok(glyph) => glyph.id,
            Err(GlyphFault::Missing(_)) => panic!("subset font does not map A"),
            Err(GlyphFault::NoAdvance) => panic!("subset font has no advance for A"),
        };
        assert_eq!(source_gid, 1);
        assert_eq!(subset_gid, source_gid);
    }

    #[test]
    fn subset_is_deterministic_and_fontfile_zeros_head_times() {
        let source = test_font();
        let first = subset_sfnt(&source, &used([(1, 'A')])).unwrap();
        let second = subset_sfnt(&source, &used([(1, 'A')])).unwrap();
        assert_eq!(first, second);

        let stamped = FontFile::from_source(&first, 0).unwrap();
        let (_, tables) = parse_sfnt(stamped.bytes()).unwrap();
        assert_eq!(&tables[b"head"][20..36], &[0; 16]);
    }

    #[test]
    fn rejects_truncated_loca_without_a_source_error() {
        let mut source = test_font();
        let record = table_record(&source, *b"loca");
        source[record + 12..record + 16].copy_from_slice(&2_u32.to_be_bytes());
        assert_head_error(&subset_sfnt(&source, &used([(1, 'A')])).unwrap_err());
    }

    #[test]
    fn rejects_a_composite_that_runs_past_its_glyph() {
        let mut source = test_font();
        let (_, tables) = parse_sfnt(&source).unwrap();
        let loca = &tables[b"loca"];
        let short = u16::from_be_bytes([loca[6], loca[7]]);
        let truncated = short.checked_add(6).unwrap();
        let record = table_record(&source, *b"loca");
        let loca_offset =
            u32::from_be_bytes(source[record + 8..record + 12].try_into().unwrap()) as usize;
        source[loca_offset + 8..loca_offset + 10].copy_from_slice(&truncated.to_be_bytes());
        assert_head_error(&subset_sfnt(&source, &used([(3, 'C')])).unwrap_err());
    }

    fn used<const N: usize>(entries: [(u16, char); N]) -> BTreeMap<u16, (char, f32)> {
        entries
            .into_iter()
            .map(|(gid, scalar)| (gid, (scalar, 500.0)))
            .collect()
    }

    fn assert_head_error(error: &crate::error::Error) {
        assert_eq!(
            error.kind,
            ErrorKind::FontNotConfigured {
                reason: FontReason::Head,
            }
        );
        assert!(std::error::Error::source(&error).is_none());
    }

    fn test_font() -> Vec<u8> {
        let glyph0 = simple_glyph();
        let glyph1 = simple_glyph();
        let glyph2 = simple_glyph();
        let glyph3 = composite_glyph();
        let all_glyphs = [glyph0, glyph1, glyph2, glyph3];
        let mut glyf = Vec::new();
        let mut offsets = Vec::new();
        for glyph in all_glyphs {
            offsets.push(glyf.len());
            glyf.extend_from_slice(&glyph);
            if glyf.len() % 2 != 0 {
                glyf.push(0);
            }
        }
        offsets.push(glyf.len());
        while glyf.len() % 4 != 0 {
            glyf.push(0);
        }

        let mut loca = Vec::new();
        for offset in offsets {
            let value = u16::try_from(offset / 2).unwrap();
            loca.extend_from_slice(&value.to_be_bytes());
        }
        let mut head = vec![0; 54];
        head[0..4].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
        head[4..8].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
        head[12..16].copy_from_slice(&0x5F0F_3CF5_u32.to_be_bytes());
        head[18..20].copy_from_slice(&1000_u16.to_be_bytes());
        head[20..36].fill(0x55);
        head[50..52].copy_from_slice(&0_i16.to_be_bytes());

        let mut maxp = vec![0; 32];
        maxp[..4].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
        maxp[4..6].copy_from_slice(&4_u16.to_be_bytes());
        let mut hhea = vec![0; 36];
        hhea[..4].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
        hhea[4..6].copy_from_slice(&800_i16.to_be_bytes());
        hhea[6..8].copy_from_slice(&(-200_i16).to_be_bytes());
        hhea[34..36].copy_from_slice(&4_u16.to_be_bytes());
        let mut hmtx = Vec::new();
        for _ in 0..4 {
            hmtx.extend_from_slice(&500_u16.to_be_bytes());
            hmtx.extend_from_slice(&0_i16.to_be_bytes());
        }
        let mut tables = BTreeMap::new();
        tables.insert(
            *b"cmap",
            build_cmap(&used([(1, 'A'), (2, 'B'), (3, 'C')])).unwrap(),
        );
        tables.insert(*b"glyf", glyf);
        tables.insert(*b"head", head);
        tables.insert(*b"hhea", hhea);
        tables.insert(*b"hmtx", hmtx);
        tables.insert(*b"loca", loca);
        tables.insert(*b"maxp", maxp);
        write_sfnt([0, 1, 0, 0], &tables).unwrap()
    }

    fn simple_glyph() -> Vec<u8> {
        let mut glyph = vec![0; 12];
        glyph[..2].copy_from_slice(&0_i16.to_be_bytes());
        glyph
    }

    fn composite_glyph() -> Vec<u8> {
        let mut glyph = vec![0; 10];
        glyph[..2].copy_from_slice(&(-1_i16).to_be_bytes());
        glyph.extend_from_slice(
            &(super::MORE_COMPONENTS | super::ARG_1_AND_2_ARE_WORDS).to_be_bytes(),
        );
        glyph.extend_from_slice(&1_u16.to_be_bytes());
        glyph.extend_from_slice(&[0; 4]);
        glyph.extend_from_slice(&super::ARG_1_AND_2_ARE_WORDS.to_be_bytes());
        glyph.extend_from_slice(&2_u16.to_be_bytes());
        glyph.extend_from_slice(&[0; 4]);
        glyph
    }

    fn loca_spans(sfnt: &[u8]) -> Vec<usize> {
        let (_, tables) = parse_sfnt(sfnt).unwrap();
        let maxp = &tables[b"maxp"];
        let count = usize::from(u16::from_be_bytes([maxp[4], maxp[5]]));
        let head = &tables[b"head"];
        let loca = &tables[b"loca"];
        let long = i16::from_be_bytes([head[50], head[51]]) == 1;
        let mut offsets = Vec::new();
        for index in 0..=count {
            let offset = if long {
                let at = index * 4;
                u32::from_be_bytes(loca[at..at + 4].try_into().unwrap()) as usize
            } else {
                let at = index * 2;
                usize::from(u16::from_be_bytes(loca[at..at + 2].try_into().unwrap())) * 2
            };
            offsets.push(offset);
        }
        offsets.windows(2).map(|pair| pair[1] - pair[0]).collect()
    }

    fn table_record(sfnt: &[u8], tag: [u8; 4]) -> usize {
        let count = usize::from(u16::from_be_bytes([sfnt[4], sfnt[5]]));
        (0..count)
            .map(|index| 12 + index * 16)
            .find(|record| sfnt[*record..*record + 4] == tag)
            .unwrap()
    }
}
