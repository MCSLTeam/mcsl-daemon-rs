// Packet format:
// 4 bytes: magic number (0x2cbb -> v1)
// varint: request body length
// varint: attachment length
// [...request body]
// [...attachment]

function encodeVarint(value: number): Uint8Array {
  const bytes: number[] = [];
  while (value >= 0x80) {
    bytes.push((value & 0x7F) | 0x80);
    value >>>= 7;
  }
  bytes.push(value & 0x7F);
  return new Uint8Array(bytes);
}

function decodeVarint(data: Uint8Array, offset: number): { value: number, newOffset: number } {
  let value = 0;
  let shift = 0;
  let currentOffset = offset;

  while (currentOffset < data.length) {
    const byte = data[currentOffset++];
    value |= (byte & 0x7F) << shift;

    if ((byte & 0x80) === 0) {
      break;
    }
    shift += 7;
  }

  return { value, newOffset: currentOffset };
}

function encodeBinaryRequest(requestBody: any, attachment?: Uint8Array): Uint8Array {
  const MAGIC_NUMBER = 0x2cbb;

  // 创建魔数字节 (4字节，小端序)
  const magicBytes = new Uint8Array(4);
  const magicView = new DataView(magicBytes.buffer);
  magicView.setUint32(0, MAGIC_NUMBER, true); // little endian

  requestBody = new TextEncoder().encode(JSON.stringify(requestBody));

  // 编码长度
  const requestBodyLength = encodeVarint(requestBody.length);
  const attachmentLength = encodeVarint(attachment?.length ?? 0);

  // 计算总长度
  const totalLength = magicBytes.length + requestBodyLength.length + attachmentLength.length +
    requestBody.length + (attachment?.length ?? 0);

  // 创建结果缓冲区
  const result = new Uint8Array(totalLength);
  let offset = 0;

  // 写入魔数
  result.set(magicBytes, offset);
  offset += magicBytes.length;

  // 写入请求体长度
  result.set(requestBodyLength, offset);
  offset += requestBodyLength.length;

  // 写入附件长度
  result.set(attachmentLength, offset);
  offset += attachmentLength.length;

  // 写入请求体
  result.set(requestBody, offset);
  offset += requestBody.length;

  // 写入附件
  if (attachment) {
    result.set(attachment, offset);
  }

  return result;
}

function decodeBinaryRequest(data: Uint8Array): {
  magicNumber: number,
  requestBody: any,
  attachment: Uint8Array
} {
  let offset = 0;

  // 读取魔数 (4字节)
  if (data.length < 4) {
    throw new Error('packet too short');
  }

  const magicView = new DataView(data.buffer, data.byteOffset, 4);
  const magicNumber = magicView.getUint32(0, true); // little endian
  offset += 4;

  // 读取请求体长度
  const { value: requestBodyLength, newOffset: offset1 } = decodeVarint(data, offset);
  offset = offset1;

  // 读取附件长度
  const { value: attachmentLength, newOffset: offset2 } = decodeVarint(data, offset);
  offset = offset2;

  // 检查数据长度是否足够
  if (offset + requestBodyLength + attachmentLength > data.length) {
    throw new Error('数据包长度不足');
  }

  // 读取请求体
  const requestBody = data.slice(offset, offset + requestBodyLength);
  offset += requestBodyLength;

  // 读取附件
  const attachment = data.slice(offset, offset + attachmentLength);

  const decoder = new TextDecoder();

  return {
    magicNumber,
    requestBody: JSON.parse(decoder.decode(requestBody)),
    attachment
  };
}

function toHex(data: Uint8Array): string {
  return Array.from(data).map(b => b.toString(16).padStart(2, '0')).join('');
}

function fromHex(data: string): Uint8Array {
  return new Uint8Array(data.match(/.{1,2}/g)?.map(byte => parseInt(byte, 16)) ?? []);
}

function toBase64(data: Uint8Array): string {
  return btoa(String.fromCharCode(...data));
}

function fromBase64(data: string): Uint8Array {
  return new Uint8Array(atob(data).split('').map(c => c.charCodeAt(0)));
}

console.log(encodeBinaryRequest({
  action: "ping",
  params: {},
  id: crypto.randomUUID()
}));

console.log(decodeBinaryRequest(fromHex(toHex(encodeBinaryRequest({
  action: "ping",
  params: {},
  id: crypto.randomUUID()
})))));