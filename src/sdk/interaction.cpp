#include "Live2DCubismCore.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iostream>

extern "C"
{

  void *mocMemory;
  void *modelMemory;
  unsigned int mocSize;
  unsigned int modelSize;
  csmMoc *moc;
  std::string cargo_manifest;
  csmModel *model;

  void logPrint(const char *message) { printf("[LOG] %s\n", message); }

  void errPrint(const char *message) { printf("[ERR] %s\n", message); }

  void *AllocateAligned(size_t size, size_t alignment)
  {
#if defined(_MSC_VER) || defined(__MINGW32__)
    return _aligned_malloc(size, alignment);
#else
    void *pointer = nullptr;
    if (posix_memalign(&pointer, alignment, size) != 0)
    {
      return nullptr;
    }
    return pointer;
#endif
  }

  void *ReadBlobAligned(const char *filePath, size_t alignment,
                        unsigned int *outSize)
  {
    std::ifstream file(filePath, std::ios::binary | std::ios::ate);
    if (!file.is_open())
    {
      errPrint("Failed to open file: ");
      errPrint(filePath);
      return nullptr;
    }

    std::streamsize size = file.tellg();
    file.seekg(0, std::ios::beg);

    if (outSize)
    {
      *outSize = static_cast<unsigned int>(size);
    }

    void *alignedBuffer = AllocateAligned(static_cast<size_t>(size), alignment);
    if (!alignedBuffer)
    {
      errPrint("Memory allocation failed.");
      return nullptr;
    }

    if (!file.read(static_cast<char *>(alignedBuffer), size))
    {
      errPrint("Failed to read file data.");
      return nullptr;
    }

    return alignedBuffer;
  }

  void updateModel()
  {
    if (!model)
      return;
    if (csmOpacityDidChange || csmDrawOrderDidChange || csmRenderOrderDidChange || csmVertexPositionsDidChange || csmBlendColorDidChange)
      csmUpdateModel(model);
  }

  // ---- DRAWABLES ----

  int getDrawablesCount()
  {
    int drawableCount = csmGetDrawableCount(model);
    return drawableCount;
  }

  int getDrawableTexIndices(int drawableIndex)
  {
    return csmGetDrawableTextureIndices(model)[drawableIndex];
  }

  int getDrawableRenderingOrder(int drawableIndex)
  {
    return csmGetRenderOrders(model)[drawableIndex];
  }

  int getDrawableBlendingState(int drawableIndex)
  {
    unsigned int flags = csmGetDrawableConstantFlags(model)[drawableIndex];

    if (flags & csmBlendAdditive)
      return 1;
    if (flags & csmBlendMultiplicative)
      return 2;

    return 0;
  }

  float getDrawableOpacity(int drawableIndex)
  {
    return csmGetDrawableOpacities(model)[drawableIndex];
  }

  int isDrawableVisible(int idx)
  {
    const unsigned char flags = csmGetDrawableDynamicFlags(model)[idx];

    if (flags & csmIsVisible)
    {
      return 1;
    }
    return 0;
  }

  int getDrawableParentPartIndex(int idx)
  {
    return csmGetDrawableParentPartIndices(model)[idx];
  }

  void getDrawableGeometry(int drawableIndex, int *outVertexCount,
                           const float **outPositions, const float **outUvs,
                           int *outIndexCount,
                           const unsigned short **outIndices)
  {

    *outVertexCount = csmGetDrawableVertexCounts(model)[drawableIndex];
    *outIndexCount = csmGetDrawableIndexCounts(model)[drawableIndex];

    *outPositions = (const float *)csmGetDrawableVertexPositions(model)[drawableIndex];
    *outUvs = (const float *)csmGetDrawableVertexUvs(model)[drawableIndex];
    *outIndices = csmGetDrawableIndices(model)[drawableIndex];
  }

  // ---- PARAMETERS ----

  int getParameterCount()
  {
    return csmGetParameterCount(model);
  }

  const char **getParameterIds()
  {
    return csmGetParameterIds(model);
  }

  float getParameterValue(int id)
  {
    if (!model)
      return 0.0f;
    return csmGetParameterValues(model)[id];
  }

  int getParameterId(const char *name)
  {
    if (!model)
      return -1;

    int id = 0;
    const char **ptr = getParameterIds();

    while (*ptr != nullptr && std::strcmp(*ptr, name) != 0)
    {
      ptr++;
      id++;
    }

    if (std::strcmp(*ptr, name) != 0)
    {
      return -1;
    }

    return id;
  }

  void setParameterValue(int id, const float value)
  {
    int count = getParameterCount();
    float *values = csmGetParameterValues(model);

    values[id] = value;
  }

  // ---- MASKS ----

  int getMasksCount(int idx)
  {
    return csmGetDrawableMaskCounts(model)[idx];
  }

  const int *getMasks(int idx)
  {
    return csmGetDrawableMasks(model)[idx];
  }

  // ---- PARTS ----

  int getPartCount()
  {
    return csmGetPartCount(model);
  }

  const char **getPartIds()
  {
    return csmGetPartIds(model);
  }

  float getPartOpacity(int id)
  {
    return csmGetPartOpacities(model)[id];
  }

  void setPartOpacity(int id, float val)
  {
    float *list = csmGetPartOpacities(model);
    list[id] = val;
  }

  // ---- LOADING ----

  int load_model()
  {
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

  void init(const char *project_dir)
  {
    csmSetLogFunction(logPrint);
    cargo_manifest = project_dir;
    load_model();
  }
}
