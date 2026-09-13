#include "Live2DCubismCore.h"
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <iostream>

extern "C" {

void *mocMemory;
void *modelMemory;
unsigned int mocSize;
unsigned int modelSize;
csmMoc *moc;
std::string cargo_manifest;
csmModel *model;

void logPrint(const char *message) { printf("[LOG] %s\n", message); }

void errPrint(const char *message) { printf("[ERR] %s\n", message); }

void *AllocateAligned(size_t size, size_t alignment) {
#if defined(_MSC_VER) || defined(__MINGW32__)
  return _aligned_malloc(size, alignment);
#else
  void *pointer = nullptr;
  if (posix_memalign(&pointer, alignment, size) != 0) {
    return nullptr;
  }
  return pointer;
#endif
}

void *ReadBlobAligned(const char *filePath, size_t alignment,
                      unsigned int *outSize) {
  std::ifstream file(filePath, std::ios::binary | std::ios::ate);
  if (!file.is_open()) {
    errPrint("Failed to open file: ");
    errPrint(filePath);
    return nullptr;
  }

  std::streamsize size = file.tellg();
  file.seekg(0, std::ios::beg);

  if (outSize) {
    *outSize = static_cast<unsigned int>(size);
  }

  void *alignedBuffer = AllocateAligned(static_cast<size_t>(size), alignment);
  if (!alignedBuffer) {
    errPrint("Memory allocation failed.");
    return nullptr;
  }

  if (!file.read(static_cast<char *>(alignedBuffer), size)) {
    errPrint("Failed to read file data.");
    return nullptr;
  }

  return alignedBuffer;
}

int getDrawablesCount() {
  int drawableCount = csmGetDrawableCount(model);
  return drawableCount;
}

int getDrawableTexIndices(int drawableIndex) {
  return csmGetDrawableTextureIndices(model)[drawableIndex];
}

int getDrawableRenderingOrder(int drawableIndex) {
  return csmGetDrawableDrawOrders(model)[drawableIndex];
}

int getDrawableBlendingState(int drawableIndex) {
  unsigned int flags = csmGetDrawableConstantFlags(model)[drawableIndex];

  if (flags & csmColorBlendType_Add) return 1;
  if (flags & csmColorBlendType_Multiply) return 2;

  return 0;
}

void getDrawableGeometry(int drawableIndex, int *outVertexCount,
                         const float **outPositions, const float **outUvs,
                         int *outIndexCount,
                         const unsigned short **outIndices) {
  
  csmUpdateModel(model);

  *outVertexCount = csmGetDrawableVertexCounts(model)[drawableIndex];
  *outIndexCount = csmGetDrawableIndexCounts(model)[drawableIndex];

  *outPositions = (const float *)csmGetDrawableVertexPositions(model)[drawableIndex];
  *outUvs = (const float *)csmGetDrawableVertexUvs(model)[drawableIndex];
  *outIndices = csmGetDrawableIndices(model)[drawableIndex];
}

  int load_model() {
    mocMemory = ReadBlobAligned(
      (cargo_manifest + "/models/runtime/zundamon.moc3").c_str(), csmAlignofMoc,
      &mocSize);
    logPrint("Loaded moc3 model.");
    moc = csmReviveMocInPlace(mocMemory, mocSize);
  modelSize = csmGetSizeofModel(moc);

  modelMemory = AllocateAligned(modelSize, csmAlignofModel);

  logPrint("Trying to initialize the model...");
  model = csmInitializeModelInPlace(moc, modelMemory, modelSize);

  csmVector2 size;
  csmVector2 origin;
  float pixelsPerUnit;
  csmReadCanvasInfo(model, &size, &origin, &pixelsPerUnit);

  printf("[LOG] Canvas Size: Width = %f, Height = %f\n", size.X, size.Y);
  printf("[LOG] Canvas Origin: X = %f, Y = %f\n", origin.X, origin.Y);


  return 0;
}

void init(const char *project_dir) {
  csmSetLogFunction(logPrint);
  cargo_manifest = project_dir;
  load_model();
}
}
