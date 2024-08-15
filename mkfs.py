import sys
import pathlib
import struct

MAGIC        = 0xce3fdefe
MAX_DIR_NAME = 32

class INode:
    def __init__(self, name: str, isdir: bool, size: int, addr: int):
        if len(name) > MAX_DIR_NAME:
            raise Exception(f"name `{name}`, max is {MAX_DIR_NAME} characters")

        self.name = name
        self.dir  = isdir
        self.size = size
        self.addr = addr

    def serialize_data(self):
        return struct.pack("<HHIQ", len(self.name), int(self.dir), self.size, self.addr)

class ParentINode(INode):
    def __init__(self, parent: INode):
        self.parent = parent
        INode.__init__(self, "..", True, 0, 0)

    def serialize_data(self):
        return struct.pack("<HHIQ", len(self.name), int(self.dir), self.parent.size, self.parent.addr)

def adddir(inodes: list[INode], data: bytearray, path: pathlib.Path, parent: int) -> int:
    def append_inode(inode: INode):
        ino = len(inodes)
        inodes.append(inode)
        return ino

    ino      = append_inode(INode("" if len(inodes) == 0 else path.name, True, 0, 0))
    children = [0, 0]
    for child in path.iterdir():
        if child.is_dir():
            children.append(adddir(inodes, data, child, ino))
        elif child.is_file():
            buf = child.read_bytes()
            children.append(append_inode(INode(child.name, False, len(buf), len(data))))
            data.extend(buf)

    inodes[ino].size = len(children)
    inodes[ino].addr = len(data)

    children[0] = append_inode(INode(".", True, inodes[ino].size, inodes[ino].addr))
    children[1] = append_inode(ParentINode(inodes[parent]))
    for child in children:
        data.extend(struct.pack("<Q", child))

    return ino

def main():
    src = pathlib.Path(sys.argv[1])
    dst = sys.argv[2]
    if not src.is_dir():
        raise Exception("src is not a directory")

    inodes = []
    data   = bytearray()
    adddir(inodes, data, src, 0)
    with open(dst, "wb") as file:
        file.write(struct.pack("<IIQ", MAGIC, 0, len(inodes)))
        for node in inodes:
            file.write(node.name.encode().ljust(MAX_DIR_NAME, b"\0"))
            file.write(node.serialize_data())
        file.write(data)

if __name__ == "__main__":
    main()

